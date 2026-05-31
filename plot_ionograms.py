#!/usr/bin/env python
import re
import traceback
import time
import os
import os.path
import sys
os.environ.setdefault("MPLBACKEND", "Agg")
import chirp_det as cd
import chirp_config as cc
import h5py
import glob
import numpy as n
import psutil
import gc
import shutil
import ctypes
import datetime as dt
import subprocess
plt = None
c = None
p = psutil.Process()
# Set I/O priority to idle (lowest) to avoid interrupting realtime processes
p.ionice(psutil.IOPRIO_CLASS_IDLE)
p.nice(19)

FAILED_RETRY_SEC = 300.0
STATUS_PRINT_SEC = 60.0
REALTIME_PLOT_AGE_SEC = 48 * 3600.0
MEMORY_CHECK_MIN_SAMPLES = 12
MEMORY_LEAK_MIN_GROWTH_MB = 300.0
MEMORY_LEAK_MIN_SLOPE_MB_PER_IONOGRAM = 3.0

def log(msg):
    print(msg, flush=True)

def current_rss_mb():
    return p.memory_info().rss / 1024.0**2

def load_plot_config(conf_path):
    """Load config without allocating detection-only frequency vectors."""
    return cc.chirp_config(conf_path, build_fvec=False, verbose=False)

def config_mtime_key(conf_path):
    paths = [os.path.abspath(conf_path)]
    server_path = os.path.join(os.path.dirname(paths[0]), "server.ini")
    if server_path != paths[0]:
        paths.append(server_path)
    key = []
    for path in paths:
        try:
            key.append((path, os.path.getmtime(path)))
        except OSError:
            key.append((path, None))
    return tuple(key)

class PlotConfigCache:
    def __init__(self, conf_path):
        self.conf_path = conf_path
        self.mtime_key = None
        self.conf = None

    def get(self):
        mtime_key = config_mtime_key(self.conf_path)
        if self.conf is None or mtime_key != self.mtime_key:
            rss_before = current_rss_mb()
            self.conf = load_plot_config(self.conf_path)
            self.mtime_key = mtime_key
            log("plot_ionograms.py: reloaded config, rss %.1f -> %.1f MB" %
                (rss_before, current_rss_mb()))
        return self.conf

class MemoryGrowthMonitor:
    def __init__(self,
                 min_samples=MEMORY_CHECK_MIN_SAMPLES,
                 min_growth_mb=MEMORY_LEAK_MIN_GROWTH_MB,
                 min_slope_mb_per_ionogram=MEMORY_LEAK_MIN_SLOPE_MB_PER_IONOGRAM):
        self.min_samples = min_samples
        self.min_growth_mb = min_growth_mb
        self.min_slope_mb_per_ionogram = min_slope_mb_per_ionogram
        self.samples = []
        self.n_ionograms = 0
        self.last_stats = None

    def sample(self):
        self.n_ionograms += 1
        rss_mb = current_rss_mb()
        self.samples.append((self.n_ionograms, rss_mb))
        if len(self.samples) < self.min_samples:
            self.last_stats = {
                "rss_mb": rss_mb,
                "growth_mb": 0.0,
                "slope_mb_per_ionogram": 0.0,
                "r_value": 0.0,
                "n_samples": len(self.samples),
                "warming_up": True,
            }
            return self.last_stats

        x = n.array([sample[0] for sample in self.samples], dtype=n.float64)
        y = n.array([sample[1] for sample in self.samples], dtype=n.float64)
        x_mean = n.mean(x)
        y_mean = n.mean(y)
        denom = n.sum((x - x_mean)**2.0)
        if denom <= 0.0:
            self.last_stats = {
                "rss_mb": rss_mb,
                "growth_mb": 0.0,
                "slope_mb_per_ionogram": 0.0,
                "r_value": 0.0,
                "n_samples": len(self.samples),
                "warming_up": True,
            }
            return self.last_stats

        slope_mb_per_ionogram = n.sum((x - x_mean) * (y - y_mean)) / denom
        growth_mb = y[-1] - y[0]
        r_value = 0.0
        y_var = n.sum((y - y_mean)**2.0)
        if y_var > 0.0:
            r_value = n.sum((x - x_mean) * (y - y_mean)) / n.sqrt(denom * y_var)

        self.last_stats = {
            "rss_mb": rss_mb,
            "growth_mb": growth_mb,
            "slope_mb_per_ionogram": slope_mb_per_ionogram,
            "r_value": r_value,
            "n_samples": len(self.samples),
            "warming_up": False,
        }
        return self.last_stats

    def leaking(self, stats):
        if stats is None:
            return False
        return (
            stats["growth_mb"] >= self.min_growth_mb and
            stats["slope_mb_per_ionogram"] >= self.min_slope_mb_per_ionogram and
            stats["r_value"] >= 0.85
        )

    def format_stats(self, stats):
        if stats is None:
            return "rss unknown"
        if stats.get("warming_up", False):
            return "rss %.1f MB, memory monitor warming up %d/%d ionograms" % (
                stats["rss_mb"], stats["n_samples"], self.min_samples)
        return "rss %.1f MB, growth %.1f MB, slope %.2f MB/ionogram, r %.2f, samples %d" % (
            stats["rss_mb"],
            stats["growth_mb"],
            stats["slope_mb_per_ionogram"],
            stats["r_value"],
            stats["n_samples"])

def kill(conf):
    exists = os.path.isfile(conf.kill_path)
    return exists

def trim_process_memory():
    """Ask glibc to return free heap pages to the OS when available."""
    try:
        ctypes.CDLL("libc.so.6").malloc_trim(0)
    except Exception:
        pass

def sample_memory_after_ionogram(memory_monitor, fn):
    gc.collect()
    trim_process_memory()
    stats = memory_monitor.sample()
    log("plot_ionograms.py: memory after %s: %s" %
        (os.path.basename(fn), memory_monitor.format_stats(stats)))
    if memory_monitor.leaking(stats):
        log("WARNING: plot_ionograms.py RSS appears to grow linearly per ionogram; exiting so the supervisor can restart it. "
            "rss=%.1f MB growth=%.1f MB slope=%.2f MB/ionogram r=%.2f samples=%d" %
            (stats["rss_mb"],
             stats["growth_mb"],
             stats["slope_mb_per_ionogram"],
             stats["r_value"],
             stats["n_samples"]))
        sys.exit(2)

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
    m = re.search(
        r".*/lfm_ionogram-.*-(ch[^-]+)-(\d+)-(1\d+(?:\.\d+)?).h5$",
        fn,
    )
    if m:
        ch = m.group(1)
        cid = int(m.group(2))
        t0 = float(m.group(3))
        return "%s/%s/lfm_ionogram-%s-%03d-%1.2f.png" % (
            conf.output_dir, cd.unix2dirname(t0), ch, cid, t0)

    with h5py.File(fn, "r") as ho:
        if "id" not in ho.keys():
            return None
        t0 = float(n.copy(ho[("t0")]))
        ch = ho["ch"][()]
        ch = ch.decode('utf-8')
        cid = int(n.copy(ho[("id")]))
    return "%s/%s/lfm_ionogram-%s-%03d-%1.2f.png" % (
        conf.output_dir, cd.unix2dirname(t0), ch, cid, t0)

def ensure_plotting_imports():
    global plt, c
    if plt is None:
        import matplotlib
        matplotlib.use('Agg')
        import matplotlib.pyplot as mpl_plt
        import scipy.constants as scipy_constants
        plt = mpl_plt
        c = scipy_constants

def realtime_date_dirs(output_dir, t_now, age_sec):
    start = dt.datetime.utcfromtimestamp(t_now - age_sec - 24 * 3600.0)
    stop = dt.datetime.utcfromtimestamp(t_now + 24 * 3600.0)
    day = start.date()
    stop_day = stop.date()
    dirs = []
    while day <= stop_day:
        dirname = os.path.join(output_dir, day.strftime("%Y-%m-%d"))
        if os.path.isdir(dirname):
            dirs.append(dirname)
        day += dt.timedelta(days=1)
    return dirs

def realtime_ionogram_files(output_dir, t_now, age_sec):
    files = []
    for dirname in realtime_date_dirs(output_dir, t_now, age_sec):
        files.extend(glob.glob("%s/lfm*.h5" % (dirname)))
    files.sort()
    return files

def plot_ionogram(conf, fn, normalize_by_frequency=True):
    ensure_plotting_imports()
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
            log("Plotting %s rate %1.2f (kHz/s) t0 %1.5f (unix), rss %.1f MB" %
                (fn, rate / 1e3, t0, current_rss_mb()))
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
            range_offset_applied = bool(ho["range_offset_applied"][()]) if "range_offset_applied" in ho.keys() else False
            if range_offset_applied:
                range_start_m = float(ho["range_gate_start_m"][()]) if "range_gate_start_m" in ho.keys() else 0.0
                if n.isfinite(range_start_m) and len(ranges) > 0 and n.nanmedian(ranges) < 0.75 * range_start_m:
                    ranges = ranges + range_start_m
                range_gates = ranges / 1e3
            else:
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
            latest_txname = txname
            if txname == "unknown" and "range_gate_start_m" in ho.keys():
                try:
                    range_gate_start_m = float(ho["range_gate_start_m"][()])
                    if n.isfinite(range_gate_start_m):
                        latest_txname = "unknown-%dkm" % int(n.round(range_gate_start_m / 1e3))
                except Exception:
                    pass

            ax.set_title("%s Chirp-rate %1.2f kHz/s t0=%1.5f (unix s)\n%s-%s %s (UTC)" % (
                ch, rate / 1e3, t0, txname, station_name, cd.unix2datestr(t0)))
            ax.set_xlabel("Frequency (MHz)")
            ax.set_ylabel("One-way range offset (km)")

            if range_offset_applied and "range_gate_stop_m" in ho.keys() and "range_gate_start_m" in ho.keys():
                lower_km = n.nanmin(range_gates) if len(range_gates) else 0.0
                upper_km = float(ho["range_gate_stop_m"][()]) / 1e3
                ax.set_ylim([lower_km, upper_km])
            elif conf.manual_range_extent:
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
            latest_fname = "/tmp/latest-lfm-%s-%s.png"%(latest_txname,station_name)
            shutil.copy2(img_fname, latest_fname)
            if conf.copy_to_server:
                import ionowebsync
                response = ionowebsync.post_to_server(latest_fname)
                if response is None or not response.ok:
                    code = "no response" if response is None else "HTTP %d" % response.status_code
                    log("failed to post %s: %s" % (latest_fname, code))
            return True
    finally:
        if fig is not None:
            plt.close(fig)

def plot_ionogram_subprocess(conf_path, fn):
    """Plot one ionogram in a short-lived child process.

    This isolates matplotlib and system-library leaks seen on older Linux
    installations from the long-running realtime scanner.
    """
    cmd = [
        sys.executable,
        os.path.abspath(__file__),
        "--config",
        conf_path,
        "--plot-file",
        fn,
    ]
    return subprocess.run(cmd).returncode == 0


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Housekeeping program")
    parser.add_argument(
        "--config",
        type=str,
        default="examples/marieluise/tgo.ini",
        help="Path to configuration file"
    )
    parser.add_argument(
        "--plot-file",
        type=str,
        default=None,
        help="Plot a single ionogram file and exit"
    )
    parser.add_argument(
        "--plot-in-process",
        action="store_true",
        help="In realtime mode, plot in this process instead of a child process"
    )
    args = parser.parse_args()
    conf_path = args.config
    config_cache = PlotConfigCache(conf_path)
    conf = config_cache.get()

    if args.plot_file is not None:
        ok = plot_ionogram(conf, args.plot_file)
        gc.collect()
        trim_process_memory()
        sys.exit(0 if ok else 1)

    if conf.realtime:
        failed_files = {}
        last_status_print = 0.0
        memory_monitor = MemoryGrowthMonitor()
        while True:
            conf = config_cache.get()
            if kill(conf):
                log("kill.txt found, stopping plot_ionograms.py")
                sys.exit(0)
            else:
                t_now = time.time()
                fl = realtime_ionogram_files(
                    conf.output_dir, t_now, REALTIME_PLOT_AGE_SEC)
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
                    gc.collect()
                    trim_process_memory()
                    log("plot_ionograms.py: output_dir=%s, scanned %d recent h5 files, %d candidates newer than %.1f h, %d missing PNGs, %d recently failed, current rss %.1f MB, %s" %
                        (conf.output_dir,
                         len(fl),
                         len(candidate_files),
                         REALTIME_PLOT_AGE_SEC / 3600.0,
                         len(missing_files),
                         len(failed_files),
                         current_rss_mb(),
                         memory_monitor.format_stats(memory_monitor.last_stats)))
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
                        if args.plot_in_process:
                            plot_ok = plot_ionogram(conf, fn)
                        else:
                            plot_ok = plot_ionogram_subprocess(conf_path, fn)
                        if not plot_ok:
                            failed_files[fn] = {"mtime": mtime, "failed_at": t_now}
                        sample_memory_after_ionogram(memory_monitor, fn)

                    except:
                        failed_files[fn] = {"mtime": mtime, "failed_at": t_now}
                        log("error with %s" % (fn))
                        log(traceback.format_exc())
                        sample_memory_after_ionogram(memory_monitor, fn)
                gc.collect()
                trim_process_memory()
                time.sleep(10)
    else:
        memory_monitor = MemoryGrowthMonitor()
        fl = glob.glob("%s/*/lfm*.h5" % (conf.output_dir))
        for fn in fl:
            try:
                log("plotting %s, rss %.1f MB" % (fn, current_rss_mb()))
                plot_ionogram(conf, fn)
                sample_memory_after_ionogram(memory_monitor, fn)
                conf = config_cache.get()
            except:
                log("error with %s" % (fn))
                log(traceback.format_exc())
                sample_memory_after_ionogram(memory_monitor, fn)
