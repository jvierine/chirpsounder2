#!/usr/bin/env python3
import argparse
import glob
import os
import shutil
import sys
import time
from datetime import datetime, timezone

import chirp_config as cc
import plot_detectionfiles
import plot_digisonde
import plot_ionograms
import plot_rtf


def log(msg):
    print("%s %s" % (datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"), msg), flush=True)


def copy_if_exists(src, dst):
    if os.path.exists(src):
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copy2(src, dst)
        log("copied %s -> %s" % (src, dst))
        return True
    log("missing expected plot %s" % src)
    return False


def newest_file(pattern):
    files = glob.glob(pattern)
    if not files:
        return None
    files.sort(key=os.path.getmtime)
    return files[-1]


def newest_lfm_file(data_dir, tx, rx):
    return newest_file(os.path.join(data_dir, "2*-*-*", "lfm_ionogram-%s-%s-*.h5" % (tx, rx)))


def plot_detection_quicklook(conf, data_dir, web_dir, hours=48):
    files = plot_detectionfiles.detection_files(
        data_dir,
        max_files=int(hours / 24.0 * 96) + 16,
        station_name=conf.station_name,
    )
    dfs = plot_detectionfiles.read_detection_files(files)
    plot_end = plot_detectionfiles.newest_detection_time(dfs, time.time())
    t_start = plot_end - hours * 3600.0
    title_span = "last %d hours ending %s UTC" % (
        hours,
        datetime.fromtimestamp(plot_end, timezone.utc).strftime("%Y-%m-%d %H:%M"),
    )
    output = os.path.join(web_dir, "latest-rothr_jorn-%s.png" % conf.station_name)
    plot_detectionfiles.plot_propagation_range(
        dfs,
        t_start,
        n_hours=hours,
        min_detections=conf.min_detections,
        pfname=output,
        station_name=conf.station_name,
        title_span=title_span,
    )


def plot_lfm_latest(conf, data_dir, web_dir, tx):
    path = newest_lfm_file(data_dir, tx, conf.station_name)
    if path is None:
        log("no LFM files found for %s-%s" % (tx, conf.station_name))
        return False
    img_path = plot_ionograms.ionogram_image_name(conf, path)
    ok = plot_ionograms.plot_ionogram(conf, path)
    if not ok:
        log("plot_ionogram returned false for %s" % path)
        return False
    if img_path is not None and os.path.exists(img_path):
        return copy_if_exists(
            img_path,
            os.path.join(web_dir, "latest-lfm-%s-%s.png" % (tx, conf.station_name)),
        )
    return copy_if_exists(
        "/tmp/latest-lfm-%s-%s.png" % (tx, conf.station_name),
        os.path.join(web_dir, "latest-lfm-%s-%s.png" % (tx, conf.station_name)),
    )


def plot_digisonde_latest(data_dir, web_dir, tx, rx):
    path = plot_digisonde.latest_digisonde_file(data_dir, tx=tx, rx=rx, days=3.0)
    if path is None:
        log("no Digisonde files found for %s-%s" % (tx, rx))
        return False
    output = os.path.join(web_dir, "latest-digisonde-%s-%s.png" % (tx, rx))
    plot_digisonde.plot_digisonde_file(path, output=output, mode="auto")
    log("saved %s from %s" % (output, path))
    return True


def deploy_static_web(repo_dir, web_dir):
    src = os.path.join(repo_dir, "web")
    if not os.path.isdir(src):
        return
    for name in ("index.php", "favicon.ico", "favicon.svg", "uit-logo.png", "unis-logo-liggende.svg"):
        path = os.path.join(src, name)
        if os.path.exists(path):
            copy_if_exists(path, os.path.join(web_dir, name))


def run_once(args):
    repo_dir = os.path.dirname(os.path.abspath(__file__))
    conf = cc.chirp_config(args.config, build_fvec=False, verbose=False)
    data_dir = args.data_dir or conf.output_dir
    web_dir = args.web_dir
    os.makedirs(web_dir, exist_ok=True)

    deploy_static_web(repo_dir, web_dir)

    log("plotting RTF links for %s from %s" % (conf.station_name, data_dir))
    plot_rtf.plot_rtf_links(conf, conf.rtf_links, data_dir=data_dir)
    for tx, rx in conf.rtf_links:
        if rx == conf.station_name:
            copy_if_exists(
                "/tmp/latest-rti-%s-%s.png" % (tx, conf.station_name),
                os.path.join(web_dir, "latest-rti-%s-%s.png" % (tx, conf.station_name)),
            )

    log("plotting detection quick-look for %s" % conf.station_name)
    plot_detection_quicklook(conf, data_dir, web_dir, hours=args.hours)

    for tx in args.lfm_tx:
        log("plotting latest LFM %s-%s" % (tx, conf.station_name))
        plot_lfm_latest(conf, data_dir, web_dir, tx)

    for tx in args.digisonde_tx:
        log("plotting latest Digisonde %s-%s" % (tx, conf.station_name))
        plot_digisonde_latest(data_dir, web_dir, tx, conf.station_name)


def main():
    parser = argparse.ArgumentParser(description="Generate web quick-look plots from archived ionosonde HDF5 files")
    parser.add_argument("--config", default="examples/marieluise/kho_archive.ini")
    parser.add_argument("--data-dir", default=None)
    parser.add_argument("--web-dir", default="/var/www/html/iono")
    parser.add_argument("--interval", type=float, default=15 * 60.0)
    parser.add_argument("--hours", type=int, default=48)
    parser.add_argument("--once", action="store_true")
    parser.add_argument("--lfm-tx", action="append", default=["SGO"])
    parser.add_argument("--digisonde-tx", action="append", default=["Ramfjordmoen"])
    args = parser.parse_args()

    while True:
        try:
            run_once(args)
        except Exception:
            import traceback
            traceback.print_exc(file=sys.stdout)
        if args.once:
            return
        time.sleep(args.interval)


if __name__ == "__main__":
    main()
