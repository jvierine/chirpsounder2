#!/usr/bin/env python
import re
import traceback
import time
import os
import os.path
import sys
import chirp_det as cd
import chirp_config as cc
import scipy.constants as c
import h5py
import glob
import matplotlib
matplotlib.use('Agg')
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

def log(msg):
    print(msg, flush=True)

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
        while True:
            conf = cc.chirp_config(conf_path)
            if kill(conf):
                log("kill.txt found, stopping plot_ionograms.py")
                sys.exit(0)
            else:
                fl = glob.glob("%s/*/lfm*.h5" % (conf.output_dir))
                fl.sort()
                t_now = time.time()
                candidate_files = []
                for fn in fl[0:(len(fl) - 2)]:
                    try:
                        if t_now - ionogram_file_time(fn) < REALTIME_PLOT_AGE_SEC:
                            candidate_files.append(fn)
                    except:
                        candidate_files.append(fn)
                candidate_file_set = set(candidate_files)
                failed_files = {fn: state for fn, state in failed_files.items()
                                if fn in candidate_file_set}
                if t_now - last_status_print > STATUS_PRINT_SEC:
                    log("plot_ionograms.py: output_dir=%s, found %d h5 files, %d candidates newer than %.1f h, %d recently failed" %
                        (conf.output_dir, len(fl), len(candidate_files), REALTIME_PLOT_AGE_SEC / 3600.0, len(failed_files)))
                    last_status_print = t_now
                # avoid last file to make sure we don't read and write simultaneously
                for fn in candidate_files:
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
