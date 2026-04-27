import chirp_config as cc
import collections
import os
import re
import subprocess
import time
import matplotlib.pyplot as plt
import numpy as n


def get_cpu_temperature_c() -> float:
    out = subprocess.check_output(["sensors"], text=True)
    temps = []
    for line in out.splitlines():
        match = re.search(r"\+([0-9]+(?:\.[0-9]+)?)°C", line)
        if match:
            temps.append(float(match.group(1)))
    if not temps:
        raise RuntimeError("could not parse CPU temperature from sensors output")
    return max(temps)


def get_disk_usage_percent(path: str) -> float:
    out = subprocess.check_output(["df", "-h", path], text=True)
    lines = [line for line in out.splitlines() if line.strip()]
    if len(lines) < 2:
        raise RuntimeError("unexpected df -h output")
    parts = lines[1].split()
    usep = parts[4].strip()
    return float(usep.rstrip("%"))


def save_pc_status_plot(conf, t_hist, temp_hist, disk_hist):
    station = conf.station_name
    out_path = f"/tmp/latest-{station}-pc.png"
    t_arr = n.array(t_hist, dtype=float)
    temp_arr = n.array(temp_hist, dtype=float)
    disk_arr = n.array(disk_hist, dtype=float)
    t_minutes = (t_arr - t_arr[-1]) / 60.0 if len(t_arr) else n.array([])

    fig, axes = plt.subplots(2, 1, figsize=(6, 4.5), dpi=120, constrained_layout=True)
    axes[0].plot(t_minutes, temp_arr, color="tab:red", linewidth=1.5)
    axes[0].set_ylabel("CPU temp (C)")
    axes[0].set_title(f"{station} PC status")
    axes[0].grid(True, alpha=0.3)

    axes[1].plot(t_minutes, disk_arr, color="tab:blue", linewidth=1.5)
    axes[1].set_ylabel("Disk use (%)")
    axes[1].set_xlabel("Minutes from now")
    axes[1].set_ylim(0, 100)
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
    fig.savefig(out_path)
    plt.close(fig)


def update_pc_status(conf, t_hist, temp_hist, disk_hist):
    now = time.time()
    try:
        cpu_temp_c = get_cpu_temperature_c()
    except Exception as e:
        print(f"sensors failed: {e}")
        cpu_temp_c = n.nan
    try:
        disk_usage_percent = get_disk_usage_percent(conf.output_dir)
    except Exception as e:
        print(f"df -h failed: {e}")
        disk_usage_percent = n.nan

    t_hist.append(now)
    temp_hist.append(cpu_temp_c)
    disk_hist.append(disk_usage_percent)
    save_pc_status_plot(conf, t_hist, temp_hist, disk_hist)

def housekeeping(conf):
    t_hist = collections.deque(maxlen=24 * 60)
    temp_hist = collections.deque(maxlen=24 * 60)
    disk_hist = collections.deque(maxlen=24 * 60)
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
            time.sleep(1)
        update_pc_status(conf, t_hist, temp_hist, disk_hist)
        time.sleep(60)


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
