#!/usr/bin/env python3
import argparse
import glob
import json
import os
import re
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
        try:
            shutil.copy2(src, dst)
            log("copied %s -> %s" % (src, dst))
            return True
        except PermissionError as exc:
            try:
                os.unlink(dst)
                shutil.copy2(src, dst)
                log("replaced %s with %s" % (dst, src))
                return True
            except PermissionError:
                log("could not copy %s -> %s: %s" % (src, dst, exc))
                return False
    log("missing expected plot %s" % src)
    return False


def newest_file(patterns):
    if isinstance(patterns, str):
        patterns = [patterns]
    newest = None
    newest_t = -1.0
    for pattern in patterns:
        for path in glob.glob(pattern):
            match = re.search(r"-(1\d+(?:\.\d+)?)\.h5$", os.path.basename(path))
            if match:
                t_file = float(match.group(1))
            else:
                t_file = os.path.getmtime(path)
            if t_file > newest_t:
                newest = path
                newest_t = t_file
    return newest


def latest_archive_date_dir(data_dir):
    newest = None
    try:
        with os.scandir(data_dir) as entries:
            for entry in entries:
                if not entry.is_dir():
                    continue
                if not re.match(r"^\d{4}-\d{2}-\d{2}$", entry.name):
                    continue
                if newest is None or entry.name > newest:
                    newest = entry.name
    except FileNotFoundError:
        return None
    return newest


def state_path(web_dir, station_name):
    return os.path.join(web_dir, ".plot_archive_quicklooks_state-%s.json" % station_name)


def load_state(path):
    try:
        with open(path, "r") as f:
            state = json.load(f)
        if isinstance(state, dict):
            return state
    except FileNotFoundError:
        pass
    except Exception as exc:
        log("could not read state file %s: %s" % (path, exc))
    return {}


def save_state(path, state):
    tmp = "%s.tmp" % path
    with open(tmp, "w") as f:
        json.dump(state, f, sort_keys=True, indent=2)
        f.write("\n")
    os.replace(tmp, path)


def file_token(path):
    if path is None:
        return None
    st = os.stat(path)
    return {
        "path": path,
        "mtime_ns": st.st_mtime_ns,
        "size": st.st_size,
    }


def input_changed(state, key, path, force=False):
    token = file_token(path)
    if token is None:
        return False, None
    if force:
        return True, token
    return state.get(key) != token, token


def station_file(data_dir, date_dir, pattern):
    if date_dir is None:
        return None
    return newest_file(os.path.join(data_dir, date_dir, pattern))


def plot_detection_quicklook(conf, data_dir, web_dir, date_dir, hours=48):
    files = sorted(
        glob.glob(
            os.path.join(
                data_dir,
                date_dir,
                "cdetections-%s-*.h5" % conf.station_name,
            )
        )
    )
    max_files = int(hours / 24.0 * 96) + 16
    files = files[-max_files:]
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


def plot_lfm_latest(conf, data_dir, web_dir, tx, date_dir):
    path = station_file(
        data_dir,
        date_dir,
        "lfm_ionogram-%s-%s-*.h5" % (tx, conf.station_name),
    )
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


def plot_digisonde_latest(data_dir, web_dir, tx, rx, date_dir):
    path = station_file(
        data_dir,
        date_dir,
        "digisonde_ionogram-%s-%s-*.h5" % (tx, rx),
    )
    if path is None:
        log("no Digisonde files found for %s-%s" % (tx, rx))
        return False
    output = os.path.join(web_dir, "latest-digisonde-%s-%s.png" % (tx, rx))
    plot_digisonde.plot_digisonde_file(path, output=output, mode="auto")
    log("saved %s from %s" % (output, path))
    return True


def plot_rtf_latest_files(conf, data_dir, web_dir, tx, rx, date_dir, max_files):
    files = plot_rtf.get_ionogram_files(data_dir, tx, rx, dirname=date_dir)
    files = files[-max_files:]
    title_span = "latest %d ionograms from %s UTC" % (len(files), date_dir)
    plot_rtf.plot_ionogram_files(
        files,
        tx,
        rx,
        pfname="/tmp/latest-rti-%s-%s.png" % (tx, conf.station_name),
        title_span=title_span,
    )
    if rx != conf.station_name:
        return False
    return copy_if_exists(
        "/tmp/latest-rti-%s-%s.png" % (tx, conf.station_name),
        os.path.join(web_dir, "latest-rti-%s-%s.png" % (tx, conf.station_name)),
    )


def deploy_static_web(repo_dir, web_dir):
    """Refresh optional static assets when permissions allow it."""
    src = os.path.join(repo_dir, "web")
    if not os.path.isdir(src):
        return
    for name in ("index.php", "favicon.ico", "favicon.svg", "uit-logo.png", "unis-logo-liggende.svg"):
        path = os.path.join(src, name)
        if os.path.exists(path):
            copy_if_exists(path, os.path.join(web_dir, name))


def run_once(args):
    conf = cc.chirp_config(args.config, build_fvec=False, verbose=False)
    data_dir = args.data_dir or conf.output_dir
    web_dir = args.web_dir
    os.makedirs(web_dir, exist_ok=True)
    date_dir = latest_archive_date_dir(data_dir)
    if date_dir is None:
        log("no archive date directories found under %s" % data_dir)
        return
    log("using latest archive directory %s/%s" % (data_dir, date_dir))
    state_file = state_path(web_dir, conf.station_name)
    state = load_state(state_file)

    if args.deploy_static:
        repo_dir = os.path.dirname(os.path.abspath(__file__))
        deploy_static_web(repo_dir, web_dir)

    for tx in args.lfm_tx:
        trigger = station_file(
            data_dir,
            date_dir,
            "lfm_ionogram-%s-%s-*.h5" % (tx, conf.station_name),
        )
        key = "lfm:%s:%s" % (tx, conf.station_name)
        changed, token = input_changed(state, key, trigger, force=args.force)
        if not changed:
            log("skipping unchanged LFM %s-%s" % (tx, conf.station_name))
            continue
        log("plotting latest LFM %s-%s" % (tx, conf.station_name))
        if plot_lfm_latest(conf, data_dir, web_dir, tx, date_dir):
            state[key] = token

    for tx in args.digisonde_tx:
        trigger = station_file(
            data_dir,
            date_dir,
            "digisonde_ionogram-%s-%s-*.h5" % (tx, conf.station_name),
        )
        key = "digisonde:%s:%s" % (tx, conf.station_name)
        changed, token = input_changed(state, key, trigger, force=args.force)
        if not changed:
            log("skipping unchanged Digisonde %s-%s" % (tx, conf.station_name))
            continue
        log("plotting latest Digisonde %s-%s" % (tx, conf.station_name))
        if plot_digisonde_latest(data_dir, web_dir, tx, conf.station_name, date_dir):
            state[key] = token

    for tx, rx in conf.rtf_links:
        trigger = station_file(data_dir, date_dir, "*_ionogram-%s-%s-*.h5" % (tx, rx))
        key = "rtf:%s:%s" % (tx, rx)
        changed, token = input_changed(state, key, trigger, force=args.force)
        if not changed:
            log("skipping unchanged RTF %s-%s" % (tx, rx))
            continue
        log("plotting RTF %s-%s for %s" % (tx, rx, date_dir))
        if plot_rtf_latest_files(conf, data_dir, web_dir, tx, rx, date_dir, args.rtf_max_files):
            state[key] = token

    trigger = station_file(data_dir, date_dir, "cdetections-%s-*.h5" % conf.station_name)
    key = "detections:%s" % conf.station_name
    changed, token = input_changed(state, key, trigger, force=args.force)
    if not changed:
        log("skipping unchanged detection quick-look for %s" % conf.station_name)
    else:
        log("plotting detection quick-look for %s" % conf.station_name)
        plot_detection_quicklook(conf, data_dir, web_dir, date_dir, hours=args.hours)
        state[key] = token
    save_state(state_file, state)


def main():
    parser = argparse.ArgumentParser(description="Generate web quick-look plots from archived ionosonde HDF5 files")
    parser.add_argument("--config", default="examples/marieluise/kho_archive.ini")
    parser.add_argument("--data-dir", default=None)
    parser.add_argument("--web-dir", default="/var/www/html/iono")
    parser.add_argument("--interval", type=float, default=15 * 60.0)
    parser.add_argument("--hours", type=int, default=48)
    parser.add_argument("--rtf-max-files", type=int, default=24, help="Maximum newest ionograms per link to use for RTI/RTF plots")
    parser.add_argument("--once", action="store_true")
    parser.add_argument("--deploy-static", action="store_true")
    parser.add_argument("--force", action="store_true", help="Regenerate plots even when the newest input file is unchanged")
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
