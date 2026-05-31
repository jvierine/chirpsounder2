#!/usr/bin/env python3

import argparse
import concurrent.futures
import glob
import os
import sys
import time
import traceback

import digital_rf as drf
import h5py
import numpy as np

import calc_ionograms
import chirp_config as cc
import chirp_det as cd
import chirpsounder_version as csversion
import find_timings


def log(msg):
    print("%s %s" % (cd.unix2datestr(time.time()), msg), flush=True)


def date_dirs(conf, lookback_days=1):
    now = time.time()
    dirs = []
    for day in range(lookback_days + 1):
        dname = os.path.join(conf.output_dir, cd.unix2dirname(now - day * 86400.0))
        if os.path.isdir(dname):
            dirs.append(dname)
    return dirs


def par_files(conf):
    files = []
    for dname in date_dirs(conf):
        files.extend(glob.glob(os.path.join(dname, "par-*.h5")))
    files.sort()
    return files


def read_par(path):
    with h5py.File(path, "r") as h:
        return {
            "t0": float(h["t0"][()]),
            "chirp_rate": float(h["chirp_rate"][()]),
            "channel": h["channel"][()].decode("utf-8") if hasattr(h["channel"][()], "decode") else str(h["channel"][()]),
        }


def mark_done(path, status, reason=None):
    done = "%s.done" % path
    if os.path.exists(done):
        return
    with h5py.File(done, "w") as h:
        csversion.tag_hdf5(h)
        h["t_an"] = time.time()
        h["status"] = status
        if reason:
            h["reason"] = reason


def try_claim(path):
    lock = "%s.lock" % path
    try:
        fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
    except FileExistsError:
        return False
    with os.fdopen(fd, "w") as fh:
        fh.write("%d %.6f\n" % (os.getpid(), time.time()))
    return True


def release_claim(path):
    try:
        os.unlink("%s.lock" % path)
    except FileNotFoundError:
        pass


def raw_start_available(conf, bounds, par):
    t0 = par["t0"]
    chirp_rate = par["chirp_rate"]
    if conf.manual_freq_extent:
        min_freq = conf.min_freq
    else:
        min_freq = conf.minimum_analysis_frequency
    range_start_m = 0.0
    if conf.serendipitous and conf.serendipitous_range_quantization_km > 0:
        delay_km = (t0 - np.floor(t0)) * 299792458.0 / 1e3
        range_start_km = np.floor(delay_km / conf.serendipitous_range_quantization_km) * conf.serendipitous_range_quantization_km
        range_start_m = (range_start_km - conf.serendipitous_range_buffer_km) * 1e3
    required_t0 = t0 + min_freq / chirp_rate + range_start_m / 299792458.0
    i0 = int(required_t0 * conf.sample_rate)
    buffer_start = int(bounds[0])
    buffer_stop = int(bounds[1])
    if i0 < buffer_start:
        return False, "start_lost buffer_start=%.2f required_start=%.2f t0=%.2f" % (
            buffer_start / conf.sample_rate, required_t0, t0)
    if i0 >= buffer_stop:
        return False, "start_not_recorded_yet buffer_stop=%.2f required_start=%.2f t0=%.2f" % (
            buffer_stop / conf.sample_rate, required_t0, t0)
    return True, ""


def worker(conf_path, par_path):
    conf = cc.chirp_config(conf_path)
    par = read_par(par_path)
    d = drf.DigitalRFReader(conf.data_dir)
    ch = par["channel"]
    bounds = calc_ionograms.get_valid_bounds(d, ch)
    ok, reason = raw_start_available(conf, bounds, par)
    if not ok:
        if reason.startswith("start_lost"):
            mark_done(par_path, "abandoned", reason)
            return "abandoned %s %s" % (par_path, reason)
        raise RuntimeError(reason)

    i0 = np.int64(par["t0"] * conf.sample_rate)
    log("worker analyzing %s rate %.2f kHz/s" % (par_path, par["chirp_rate"] / 1e3))
    calc_ionograms.chirp_downconvert(
        conf,
        par["t0"],
        d,
        i0,
        ch,
        par["chirp_rate"],
        dec=conf.decimation,
        txname="unknown",
        cid=0,
    )
    mark_done(par_path, "done")
    return "done %s" % par_path


def submit_ready_jobs(conf, conf_path, executor, futures):
    d = drf.DigitalRFReader(conf.data_dir)
    bounds_by_channel = {}

    for path in par_files(conf):
        if len(futures) >= conf.serendipitous_ionogram_workers:
            return
        if os.path.exists("%s.done" % path) or os.path.exists("%s.lock" % path):
            continue
        try:
            par = read_par(path)
            ch = par["channel"]
            if ch not in bounds_by_channel:
                bounds_by_channel[ch] = calc_ionograms.get_valid_bounds(d, ch)
            ok, reason = raw_start_available(conf, bounds_by_channel[ch], par)
        except Exception:
            log("could not inspect %s" % path)
            traceback.print_exc(file=sys.stdout)
            continue

        if not ok:
            if reason.startswith("start_lost"):
                log("abandoning %s: %s" % (path, reason))
                mark_done(path, "abandoned", reason)
            continue

        if try_claim(path):
            log("queued %s" % path)
            futures[executor.submit(worker, conf_path, path)] = path


def reap_finished(futures):
    for fut in list(futures):
        if not fut.done():
            continue
        path = futures.pop(fut)
        try:
            log(fut.result())
        except Exception as e:
            log("job failed %s: %s" % (path, e))
            traceback.print_exc(file=sys.stdout)
            release_claim(path)


def main():
    parser = argparse.ArgumentParser(description="Queue serendipitous ionogram calculations from recent chirp detections.")
    parser.add_argument("--config", default="examples/marieluise/w2naf.ini")
    parser.add_argument("--scan-interval", type=float, default=5.0)
    args = parser.parse_args()

    conf = cc.chirp_config(args.config)
    conf_path = os.path.abspath(args.config)
    log("starting serendipitous queue with %d workers" % conf.serendipitous_ionogram_workers)

    futures = {}
    with concurrent.futures.ProcessPoolExecutor(max_workers=conf.serendipitous_ionogram_workers) as executor:
        while True:
            if os.path.isfile(conf.kill_path):
                log("kill.txt found, stopping serendipitous queue")
                return
            try:
                for ch in conf.channel:
                    find_timings.scan_for_chirps(conf, ch)
                submit_ready_jobs(conf, conf_path, executor, futures)
                reap_finished(futures)
            except KeyboardInterrupt:
                raise
            except Exception:
                log("queue loop error")
                traceback.print_exc(file=sys.stdout)
            time.sleep(args.scan_interval)


if __name__ == "__main__":
    main()
