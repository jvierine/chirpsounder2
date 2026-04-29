import chirp_config as cc
import collections
import os
import re
import shutil
import subprocess
import time
import matplotlib.pyplot as plt
import numpy as n


def get_cpu_temperatures_c() -> dict:
    out = subprocess.check_output(["sensors"], text=True)
    temps = {}
    in_coretemp_block = False
    for line in out.splitlines():
        stripped = line.strip()
        if stripped.startswith("coretemp-isa-"):
            in_coretemp_block = True
            continue
        if in_coretemp_block and stripped.startswith("Adapter:"):
            continue
        if in_coretemp_block and stripped and ":" not in stripped and not stripped.startswith("coretemp-isa-"):
            break
        if not in_coretemp_block:
            continue
        match = re.search(r"^([^:]+):\s+\+([0-9]+(?:\.[0-9]+)?)°C", stripped)
        if match:
            temps[match.group(1).strip()] = float(match.group(2))
    if not temps:
        raise RuntimeError("could not parse coretemp CPU temperatures from sensors output")
    return temps


def get_disk_usage_percent(path: str) -> float:
    out = subprocess.check_output(["df", "-h", path], text=True)
    lines = [line for line in out.splitlines() if line.strip()]
    if len(lines) < 2:
        raise RuntimeError("unexpected df -h output")
    parts = lines[1].split()
    usep = parts[4].strip()
    return float(usep.rstrip("%"))


def get_gpsdo_lock_status() -> dict:
    if shutil.which("lbe-142x") is None:
        return {}
    out = subprocess.check_output(["lbe-142x", "--status"], text=True, timeout=10)
    status = {}
    for label in ["GPS Lock", "PLL Lock"]:
        match = re.search(r"^\s*%s:\s*(Yes|No)\s*$" % re.escape(label), out, re.MULTILINE)
        if match:
            status[label] = 100.0 if match.group(1) == "Yes" else 0.0
    return status


def save_pc_status_plot(conf, t_hist, temp_histories, disk_hist, gpsdo_histories):
    station = conf.station_name
    out_path = f"/tmp/latest-{station}-pc.png"
    t_arr = n.array(t_hist, dtype=float)
    disk_arr = n.array(disk_hist, dtype=float)
    t_hours = (t_arr - t_arr[-1]) / 3600.0 if len(t_arr) else n.array([])
    keep = t_hours >= -24.0 if len(t_hours) else n.array([], dtype=bool)

    fig, axes = plt.subplots(3, 1, figsize=(6, 6.0), dpi=120, constrained_layout=True)
    for label in sorted(temp_histories.keys()):
        temp_arr = n.array(temp_histories[label], dtype=float)
        if temp_arr.size != keep.size:
            continue
        axes[0].plot(t_hours[keep], temp_arr[keep], linewidth=1.2, label=label)
    axes[0].set_ylabel("CPU temp (C)")
    axes[0].set_title(f"{station} PC status")
    axes[0].set_xlim(-24, 0)
    axes[0].grid(True, alpha=0.3)
    if temp_histories:
        axes[0].legend(loc="upper left", fontsize=7, ncols=2)

    axes[1].plot(t_hours[keep], disk_arr[keep], color="tab:blue", linewidth=1.5)
    axes[1].set_ylabel("Disk use (%)")
    axes[1].set_xlabel("Hours from now")
    axes[1].set_ylim(0, 100)
    axes[1].set_xlim(-24, 0)
    axes[1].grid(True, alpha=0.3)
    axes[1].text(
        0.02,
        0.92,
        f"disk: {conf.output_dir}",
        transform=axes[1].transAxes,
        fontsize=8,
        va="top",
        ha="left",
    )

    for label in sorted(gpsdo_histories.keys()):
        gpsdo_arr = n.array(gpsdo_histories[label], dtype=float)
        if gpsdo_arr.size != keep.size:
            continue
        axes[2].step(t_hours[keep], gpsdo_arr[keep], where="post", linewidth=1.5, label=label)
    axes[2].set_ylabel("GPSDO lock")
    axes[2].set_xlabel("Hours from now")
    axes[2].set_ylim(-5, 105)
    axes[2].set_yticks([0, 100])
    axes[2].set_yticklabels(["No", "Yes"])
    axes[2].set_xlim(-24, 0)
    axes[2].grid(True, alpha=0.3)
    if gpsdo_histories:
        axes[2].legend(loc="lower left", fontsize=7, ncols=2)
    else:
        axes[2].text(
            0.02,
            0.92,
            "lbe-142x not found",
            transform=axes[2].transAxes,
            fontsize=8,
            va="top",
            ha="left",
        )
    fig.savefig(out_path)
    plt.close(fig)


def update_pc_status(conf, t_hist, temp_histories, disk_hist, gpsdo_histories):
    now = time.time()
    try:
        cpu_temps = get_cpu_temperatures_c()
    except Exception as e:
        print(f"sensors failed: {e}")
        cpu_temps = {}
    try:
        disk_usage_percent = get_disk_usage_percent(conf.output_dir)
    except Exception as e:
        print(f"df -h failed: {e}")
        disk_usage_percent = n.nan
    try:
        gpsdo_status = get_gpsdo_lock_status()
    except Exception as e:
        print(f"lbe-142x --status failed: {e}")
        gpsdo_status = {}

    prev_len = len(t_hist)
    t_hist.append(now)
    disk_hist.append(disk_usage_percent)
    for label in cpu_temps.keys():
        if label not in temp_histories:
            temp_histories[label] = collections.deque([n.nan] * prev_len, maxlen=24 * 60)
    for label in list(temp_histories.keys()):
        temp_histories[label].append(cpu_temps.get(label, n.nan))
    for label in gpsdo_status.keys():
        if label not in gpsdo_histories:
            gpsdo_histories[label] = collections.deque([n.nan] * prev_len, maxlen=24 * 60)
    for label in list(gpsdo_histories.keys()):
        gpsdo_histories[label].append(gpsdo_status.get(label, n.nan))
    save_pc_status_plot(conf, t_hist, temp_histories, disk_hist, gpsdo_histories)

def housekeeping(conf):
    t_hist = collections.deque(maxlen=24 * 60)
    temp_histories = {}
    disk_hist = collections.deque(maxlen=24 * 60)
    gpsdo_histories = {}
    pc_status_period_s = 60
    next_pc_status_time = 0.0
    while True:
        if conf.ringbuffer_cleanup:
            print("cleaning files older than %d"%(conf.ringbuffer_max_age_min))
            print("run the following command")
            #find /dev/shm/hf25 -type f -name 'rf*h5' -mmin +5 -delete
            cmd="find %s -type f -mmin +%d -name 'rf*.h5' -delete"%(conf.data_dir,conf.ringbuffer_max_age_min)
            print(cmd)
            os.system(cmd)            
            cmd="find %s -type f -mmin +%d -name 'tmp*rf*.h5' -delete"%(conf.data_dir,conf.ringbuffer_max_age_min)
            print(cmd)            
            os.system(cmd)
        now = time.time()
#        try:
 #           if now >= next_pc_status_time:
  #              update_pc_status(conf, t_hist, temp_histories, disk_hist, gpsdo_histories)
   #             next_pc_status_time = now + pc_status_period_s
    #    except:
     #       print("plotting crapped out. fix fix fix, but it doesn't affect ringbuffer cleanup.")
        time.sleep(1)


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Housekeeping program")
    parser.add_argument(
        "--config",
        type=str,
        default="examples/marieluise/tgo.ini",
        help="Path to configuration file"
    )
    args = parser.parse_args()
    conf=cc.chirp_config(args.config)
    housekeeping(conf)
