#!/usr/bin/env python3
import argparse
import datetime as dt
import glob
import os
import re
import shutil

os.environ.setdefault("MPLBACKEND", "Agg")

import h5py
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as n
import scipy.constants as sc

import chirp_det as cd


def _decode(value):
    if hasattr(value, "decode"):
        return value.decode("utf-8")
    return str(value)


def file_time(path):
    match = re.search(r"-(1\d+(?:\.\d+)?)\.h5$", os.path.basename(path))
    if match:
        return float(match.group(1))
    with h5py.File(path, "r") as h:
        return float(h["t0"][()])


def find_digisonde_files(data_dir, tx=None, rx=None, days=3):
    if tx and rx:
        basename = "digisonde_ionogram-%s-%s-*.h5" % (tx, rx)
    elif tx:
        basename = "digisonde_ionogram-%s-*.h5" % (tx)
    elif rx:
        basename = "digisonde_ionogram-*-%s-*.h5" % (rx)
    else:
        basename = "digisonde_ionogram-*.h5"

    patterns = [os.path.join(data_dir, basename)]
    if days is None:
        patterns.append(os.path.join(data_dir, "2*-*-*", basename))
    else:
        today = dt.datetime.utcnow().date()
        for day_offset in range(int(days) + 2):
            day = today - dt.timedelta(days=day_offset)
            patterns.append(os.path.join(data_dir, day.strftime("%Y-%m-%d"), basename))

    files = []
    for pattern in patterns:
        files.extend(glob.glob(pattern))
    files = sorted(set(files), key=lambda path: file_time_from_name(path) or os.path.getmtime(path))
    if days is not None and files:
        newest = file_time(files[-1])
        cutoff = newest - days * 24 * 3600
        files = [path for path in files if file_time(path) >= cutoff]
    return files


def latest_digisonde_file(data_dir, tx=None, rx=None, days=3):
    files = find_digisonde_files(data_dir, tx=tx, rx=rx, days=days)
    if not files:
        return None
    return files[-1]


def file_time_from_name(path):
    match = re.search(r"-(1\d+(?:\.\d+)?)\.h5$", os.path.basename(path))
    if match:
        return float(match.group(1))
    return None


def read_digisonde(path, mode="auto"):
    with h5py.File(path, "r") as h:
        freqs = n.array(h["freqs"][()], dtype=n.float64)
        ranges = n.array(h["ranges"][()], dtype=n.float64)
        snr = n.array(h["SNR"][()], dtype=n.float32)
        t0 = float(h["t0"][()])
        offset_us = float(h["offset_us"][()]) if "offset_us" in h else 0.0
        tx = _decode(h["transmitter"][()]) if "transmitter" in h else "unknown"
        rx = _decode(h["receiver"][()]) if "receiver" in h else "unknown"

    if snr.ndim == 3:
        if mode == "sum" or (mode == "auto" and snr.shape[0] > 1):
            snr = n.nansum(snr, axis=0)
            label = "O+X SNR (dB)"
        elif mode == "x" and snr.shape[0] > 1:
            snr = snr[1, :, :]
            label = "X-mode SNR (dB)"
        else:
            snr = snr[0, :, :]
            label = "O-mode SNR (dB)"
    else:
        label = "SNR (dB)"

    n_f = min(len(freqs), snr.shape[0])
    n_r = min(len(ranges), snr.shape[1])
    freqs = freqs[:n_f]
    ranges = ranges[:n_r] + offset_us * 1e-6 * sc.c
    snr = snr[:n_f, :n_r]
    snr[snr <= 0.0] = n.nan
    return freqs, ranges, snr, t0, tx, rx, label


def plot_digisonde_file(path, output=None, mode="auto", latest_dir=None):
    freqs, ranges, snr, t0, tx, rx, label = read_digisonde(path, mode=mode)
    db = 10.0 * n.log10(snr.T)

    fig, ax = plt.subplots(figsize=(12, 9))
    mesh = ax.pcolormesh(
        freqs / 1e6,
        ranges / 1e3,
        db,
        vmin=0,
        vmax=20,
        cmap="gist_yarg",
        shading="auto",
    )
    cb = fig.colorbar(mesh, ax=ax)
    cb.set_label(label)
    ax.set_title("Digisonde %s-%s\n%s UTC" % (tx, rx, cd.unix2datestr(t0)))
    ax.set_xlabel("Frequency (MHz)")
    ax.set_ylabel("One-way range (km)")
    fig.tight_layout()

    if output is None:
        output = "%s.png" % path
    fig.savefig(output)
    plt.close(fig)

    if latest_dir is not None:
        os.makedirs(latest_dir, exist_ok=True)
        shutil.copy2(output, os.path.join(latest_dir, "latest-digisonde-%s-%s.png" % (tx, rx)))
    return output


def main():
    parser = argparse.ArgumentParser(description="Plot stored Digisonde HDF5 ionograms")
    parser.add_argument("path", nargs="?", help="Digisonde HDF5 file to plot")
    parser.add_argument("--data-dir", default="/mnt/shovel/ionosonde")
    parser.add_argument("--tx", default=None)
    parser.add_argument("--rx", default=None)
    parser.add_argument("--latest", action="store_true", help="Plot newest matching file")
    parser.add_argument("--days", type=float, default=3.0)
    parser.add_argument("--output", default=None)
    parser.add_argument("--latest-dir", default=None)
    parser.add_argument("--mode", choices=("auto", "o", "x", "sum"), default="auto")
    args = parser.parse_args()

    path = args.path
    if args.latest or path is None:
        path = latest_digisonde_file(args.data_dir, tx=args.tx, rx=args.rx, days=args.days)
    if path is None:
        raise SystemExit("no matching Digisonde files found")
    output = plot_digisonde_file(path, output=args.output, mode=args.mode, latest_dir=args.latest_dir)
    print("saved %s from %s" % (output, path))


if __name__ == "__main__":
    main()
