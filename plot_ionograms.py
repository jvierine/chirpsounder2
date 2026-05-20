#!/usr/bin/env python
import re
import traceback
import time
import os
import os.path
import sys
os.environ.setdefault("MPLBACKEND", "Agg")
import matplotlib
matplotlib.use('Agg')
import chirp_det as cd
import chirp_config as cc
import scipy.constants as c
import h5py
import glob
import matplotlib.pyplot as plt
import numpy as n
import psutil
import gc
import shutil
import ctypes
p = psutil.Process()
# Set I/O priority to idle (lowest) to avoid interrupting realtime processes
p.ionice(psutil.IOPRIO_CLASS_IDLE)
p.nice(19)

FAILED_RETRY_SEC = 300.0
STATUS_PRINT_SEC = 60.0
REALTIME_PLOT_AGE_SEC = 48 * 3600.0
MEMORY_CHECK_MIN_SAMPLES = 12
MEMORY_CHECK_WINDOW_SEC = 30 * 60.0
MEMORY_LEAK_MIN_GROWTH_MB = 300.0
MEMORY_LEAK_MIN_SLOPE_MB_PER_HOUR = 150.0

def log(msg):
    print(msg, flush=True)

class MemoryGrowthMonitor:
    def __init__(self,
                 min_samples=MEMORY_CHECK_MIN_SAMPLES,
                 window_sec=MEMORY_CHECK_WINDOW_SEC,
                 min_growth_mb=MEMORY_LEAK_MIN_GROWTH_MB,
                 min_slope_mb_per_hour=MEMORY_LEAK_MIN_SLOPE_MB_PER_HOUR):
        self.min_samples = min_samples
        self.window_sec = window_sec
        self.min_growth_mb = min_growth_mb
        self.min_slope_mb_per_hour = min_slope_mb_per_hour
        self.samples = []

    def sample(self):
        t_now = time.time()
        rss_mb = p.memory_info().rss / 1024.0**2
        self.samples.append((t_now, rss_mb))
        self.samples = [
            sample for sample in self.samples
            if t_now - sample[0] <= self.window_sec
        ]
        if len(self.samples) < self.min_samples:
            return None

        t0 = self.samples[0][0]
        x = n.array([sample[0] - t0 for sample in self.samples], dtype=n.float64)
        y = n.array([sample[1] for sample in self.samples], dtype=n.float64)
        x_mean = n.mean(x)
        y_mean = n.mean(y)
        denom = n.sum((x - x_mean)**2.0)
        if denom <= 0.0:
            return None

        slope_mb_per_sec = n.sum((x - x_mean) * (y - y_mean)) / denom
        slope_mb_per_hour = slope_mb_per_sec * 3600.0
        growth_mb = y[-1] - y[0]
        r_value = 0.0
        y_var = n.sum((y - y_mean)**2.0)
        if y_var > 0.0:
            r_value = n.sum((x - x_mean) * (y - y_mean)) / n.sqrt(denom * y_var)

        return {
            "rss_mb": rss_mb,
            "growth_mb": growth_mb,
            "slope_mb_per_hour": slope_mb_per_hour,
            "r_value": r_value,
            "n_samples": len(self.samples),
        }

    def leaking(self, stats):
        if stats is None:
            return False
        return (
            stats["growth_mb"] >= self.min_growth_mb and
            stats["slope_mb_per_hour"] >= self.min_slope_mb_per_hour and
            stats["r_value"] >= 0.85
        )

def kill(conf):
    exists = os.path.isfile(conf.kill_path)
    return exists

def trim_process_memory():
    """Ask glibc to return free heap pages to the OS when available."""
    try:
        ctypes.CDLL("libc.so.6").malloc_trim(0)
    except Exception:
        pass

def ionogram_file_time(fn):
    m = re.search(r".*-(1\d+(?:\.\d+)?).h5$", fn)
    if m:
        return float(m.group(1))

    try:
        with h5py.File(fn, "r") as ho:
            if "t0" in ho.keys():
                return float(n.copy(ho["t0"]))
    except:
        pass

    return os.path.getmtime(fn)

def ionogram_image_name(conf, fn):
    with h5py.File(fn, "r") as ho:
        if "id" not in ho.keys():
            return None
        t0 = float(n.copy(ho[("t0")]))
        ch = ho["ch"][()]
        ch = ch.decode('utf-8')
        cid = int(n.copy(ho[("id")]))
    return "%s/%s/lfm_ionogram-%s-%03d-%1.2f.png" % (
        conf.output_dir, cd.unix2dirname(t0), ch, cid, t0)

def plot_ionogram(conf, fn, normalize_by_frequency=True):
    fig = None
    try:
        with h5py.File(fn, "r") as ho:
            t0 = float(n.copy(ho[("t0")]))
            ch = ho["ch"][()]
        #    print(ch)
            ch = ch.decode('utf-8')
            if not "id" in ho.keys():
                log("id not in keys for %s" % (fn))
                return False
            cid = int(n.copy(ho[("id")]))  # ionosonde id

            img_fname = "%s/%s/lfm_ionogram-%s-%03d-%1.2f.png" % (
                conf.output_dir, cd.unix2dirname(t0), ch, cid, t0)
            if os.path.exists(img_fname):
        #        print("Ionogram plot %s already exists. Skipping" % (img_fname))
                return True

            rate = float(n.copy(ho[("rate")]))
            log("Plotting %s rate %1.2f (kHz/s) t0 %1.5f (unix)" %
                (fn, rate / 1e3, t0))
            # ionogram frequency-range
            if "SNR" in ho.keys():
                S =  n.array(ho["SNR"][()],dtype=n.float32)
                S[S <= 0.0] = 1e-3
            else:
                S =  n.array(ho["S"][()],dtype=n.float32)
                if normalize_by_frequency:
                    for i in range(S.shape[0]):
                        noise = n.nanmedian(S[i, :])
                        S[i, :] = (S[i, :] - noise) / noise
                    S[S <= 0.0] = 1e-3

            freqs = n.copy(ho[("freqs")])  # frequency bins
            ranges = n.copy(ho[("ranges")])  # range gates

            dB = n.transpose(10.0 * n.log10(S))
            if normalize_by_frequency == False:
                dB = dB - n.nanmedian(dB)

            dB[n.isnan(dB)] = 0.0
            dB[n.isfinite(dB) != True] = 0.0

            # assume that t0 is at the start of a standard unix second
            # therefore, the propagation time is anything added to a full second
            dt = (t0 - n.floor(t0))
            dr = dt * c.c / 1e3
            # converted to one-way travel time
            range_gates = dr + ranges / 1e3
            fig, ax = plt.subplots(figsize=(1.5 * 8, 1.5 * 6))
            mesh = ax.pcolormesh(freqs / 1e6, range_gates, dB,
                                 vmin=0, vmax=20.0, cmap="gist_yarg")
            cb = fig.colorbar(mesh, ax=ax)
            cb.set_label("SNR (dB)")
            if "station_name" in ho.keys():
                station_name=ho["station_name"][()].decode("utf-8")
            else:
                station_name=conf.station_name

            if "txname" in ho.keys():
                txname=ho["txname"][()].decode("utf-8")
            else:
                cr=int(rate / 1e3)
                if cr==100:
                    txname="ROTHR"
                elif cr==125:
                    txname="JORN"
                else:
                    txname="unknown"

            ax.set_title("%s Chirp-rate %1.2f kHz/s t0=%1.5f (unix s)\n%s-%s %s (UTC)" % (
                ch, rate / 1e3, t0, txname, station_name, cd.unix2datestr(t0)))
            ax.set_xlabel("Frequency (MHz)")
            ax.set_ylabel("One-way range offset (km)")

            if conf.manual_range_extent:
                ax.set_ylim([conf.min_range / 1e3, conf.max_range / 1e3])
            else:
                ax.set_ylim([dr - conf.max_range_extent / 1e3,
                             dr + conf.max_range_extent / 1e3])

            if conf.manual_freq_extent:
                ax.set_xlim([conf.min_freq / 1e6, conf.max_freq / 1e6])
            else:
                ax.set_xlim([0, conf.maximum_analysis_frequency / 1e6])
            fig.tight_layout()
            fig.savefig(img_fname)
            shutil.copy2(img_fname, "/tmp/latest-lfm-%s-%s.png"%(txname,station_name))
            return True
    finally:
        if fig is not None:
            plt.close(fig)
        gc.collect()
        trim_process_memory()
#    if conf.copy_to_server:
 #       os.system("rsync -av %s %s/latest_%s.png" %
  #                (img_fname, conf.copy_destination, conf.station_name))


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
    conf_path = args.config
    conf=cc.chirp_config(conf_path)

    if conf.realtime:
        failed_files = {}
        last_status_print = 0.0
        memory_monitor = MemoryGrowthMonitor()
        while True:
            conf = cc.chirp_config(conf_path)
            if kill(conf):
                log("kill.txt found, stopping plot_ionograms.py")
                sys.exit(0)
            else:
                fl = glob.glob("%s/*/lfm*.h5" % (conf.output_dir))
                fl.sort()
                t_now = time.time()
                candidate_records = []
                missing_records = []
                for fn in fl[0:(len(fl) - 2)]:
                    try:
                        t_file = ionogram_file_time(fn)
                        if t_now - t_file >= REALTIME_PLOT_AGE_SEC:
                            continue
                        img_fname = ionogram_image_name(conf, fn)
                        missing_plot = img_fname is None or not os.path.exists(img_fname)
                        record = (t_file, fn, missing_plot)
                        candidate_records.append(record)
                        if missing_plot:
                            missing_records.append(record)
                    except:
                        # If metadata lookup fails, try plotting so the real
                        # error is logged by plot_ionogram().
                        record = (0.0, fn, True)
                        candidate_records.append(record)
                        missing_records.append(record)
                candidate_records.sort(reverse=True)
                missing_records.sort(reverse=True)
                candidate_files = [record[1] for record in candidate_records]
                missing_files = [record[1] for record in missing_records]
                candidate_file_set = set(candidate_files)
                failed_files = {fn: state for fn, state in failed_files.items()
                                if fn in candidate_file_set}
                if t_now - last_status_print > STATUS_PRINT_SEC:
                    memory_stats = memory_monitor.sample()
                    if memory_stats is None:
                        memory_text = "memory monitor warming up"
                    else:
                        memory_text = ("rss %.1f MB, growth %.1f MB, slope %.1f MB/h, r %.2f" %
                                       (memory_stats["rss_mb"],
                                        memory_stats["growth_mb"],
                                        memory_stats["slope_mb_per_hour"],
                                        memory_stats["r_value"]))
                    log("plot_ionograms.py: output_dir=%s, found %d h5 files, %d candidates newer than %.1f h, %d missing PNGs, %d recently failed, %s" %
                        (conf.output_dir,
                         len(fl),
                         len(candidate_files),
                         REALTIME_PLOT_AGE_SEC / 3600.0,
                         len(missing_files),
                         len(failed_files),
                         memory_text))
                    if memory_monitor.leaking(memory_stats):
                        log("WARNING: plot_ionograms.py memory use appears to be growing linearly; exiting so the supervisor can restart it. "
                            "rss=%.1f MB growth=%.1f MB slope=%.1f MB/h r=%.2f samples=%d" %
                            (memory_stats["rss_mb"],
                             memory_stats["growth_mb"],
                             memory_stats["slope_mb_per_hour"],
                             memory_stats["r_value"],
                             memory_stats["n_samples"]))
                        sys.exit(2)
                    last_status_print = t_now
                # avoid last file to make sure we don't read and write simultaneously.
                # Only missing PNGs need work; existing plots are left alone.
                for fn in missing_files:
                    mtime = os.path.getmtime(fn)
                    failed_state = failed_files.get(fn)
                    if (failed_state is not None and
                            failed_state["mtime"] == mtime and
                            t_now - failed_state["failed_at"] < FAILED_RETRY_SEC):
                        continue
                    try:
                        # plot_ionogram() cheaply skips files whose PNG already
                        # exists. In realtime mode, only files newer than
                        # REALTIME_PLOT_AGE_SEC are considered.
                        if not plot_ionogram(conf, fn):
                            failed_files[fn] = {"mtime": mtime, "failed_at": t_now}

                    except:
                        failed_files[fn] = {"mtime": mtime, "failed_at": t_now}
                        log("error with %s" % (fn))
                        log(traceback.format_exc())
                time.sleep(10)
    else:
        fl = glob.glob("%s/*/lfm*.h5" % (conf.output_dir))
        for fn in fl:
            try:
                log("plotting %s" % (fn))
                plot_ionogram(conf, fn)
                conf = cc.chirp_config(conf_path)
            except:
                log("error with %s" % (fn))
                log(traceback.format_exc())
