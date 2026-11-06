#!/usr/bin/env python
#
# Plot digisonde_search.py candidate detections.
#
import argparse
import glob
import os
import re

import h5py
import matplotlib.dates as mdates
import matplotlib.pyplot as plt
import numpy as n


def read_scalar(h, key, default=None):
    if key not in h:
        return default
    v = h[key][()]
    if hasattr(v, "decode"):
        return v.decode("utf-8")
    return v


def files_from_input(path):
    path = os.path.abspath(path)
    if os.path.isfile(path):
        return [path]

    patterns = [
        os.path.join(path, "digisonde-search-*.h5"),
        os.path.join(path, "2*-*-*", "digisonde-search-*.h5"),
    ]
    files = []
    for pattern in patterns:
        files.extend(glob.glob(pattern))
    return sorted(set(files))


def parse_from_name(fname):
    bname = os.path.basename(fname)
    m = re.match(
        r"digisonde-search-(.*)-([0-9]+)ms-([0-9]+)kHz-([0-9]+)\.h5",
        bname)
    if m is None:
        return None
    return {
        "channel": m.group(1),
        "ipp_ms": int(m.group(2)),
        "freq_hz": float(m.group(3)) * 1e3,
        "i0": int(m.group(4)),
    }


def read_detection(fname):
    name_meta = parse_from_name(fname)
    with h5py.File(fname, "r") as h:
        sample_rate = float(read_scalar(h, "sample_rate", 25e6))

        i0 = read_scalar(h, "i0", None)
        if i0 is None and name_meta is not None:
            i0 = name_meta["i0"]

        t0 = read_scalar(h, "t0", None)
        if t0 is None:
            t0 = float(i0) / sample_rate

        freq = read_scalar(h, "f0", None)
        if freq is None and name_meta is not None:
            freq = name_meta["freq_hz"]

        ipp_ms = read_scalar(h, "ipp_ms", None)
        if ipp_ms is None:
            ipp = read_scalar(h, "ipp", None)
            if ipp is not None:
                ipp_ms = int(round(1e3 * float(ipp)))
        if ipp_ms is None and name_meta is not None:
            ipp_ms = name_meta["ipp_ms"]

        channel = read_scalar(h, "channel", None)
        if channel is None and name_meta is not None:
            channel = name_meta["channel"]

        return {
            "time": float(t0),
            "freq": float(freq),
            "ipp_ms": int(ipp_ms),
            "snr": float(read_scalar(h, "snr", n.nan)),
            "channel": str(channel),
            "file": fname,
        }


def collect_detections(files, min_snr=None, channel=None, ipp_ms=None):
    detections = []
    for fname in files:
        try:
            det = read_detection(fname)
        except Exception as e:
            print("skipping %s: %s" % (fname, e))
            continue

        if min_snr is not None and det["snr"] < min_snr:
            continue
        if channel is not None and det["channel"] != channel:
            continue
        if ipp_ms is not None and det["ipp_ms"] != ipp_ms:
            continue
        detections.append(det)
    return detections


def plot_detections(detections, output=None, title=None):
    times = n.array([d["time"] for d in detections], dtype=n.float64)
    freqs = n.array([d["freq"] for d in detections], dtype=n.float64)
    snrs = n.array([d["snr"] for d in detections], dtype=n.float64)
    ipps = n.array([d["ipp_ms"] for d in detections], dtype=n.int32)

    order = n.argsort(times)
    times = times[order]
    freqs = freqs[order]
    snrs = snrs[order]
    ipps = ipps[order]

    times_dt = times.astype("datetime64[s]").astype(object)
    snr_db = 10.0 * n.log10(n.maximum(snrs, 1e-12))

    fig, ax = plt.subplots(figsize=(12, 6), constrained_layout=True)
    markers = {5: "o", 10: "s"}
    scatter = None

    for ipp in n.unique(ipps):
        idx = n.where(ipps == ipp)[0]
        marker = markers.get(int(ipp), ".")
        scatter = ax.scatter(
            times_dt[idx],
            freqs[idx] / 1e6,
            c=snr_db[idx],
            marker=marker,
            s=18,
            alpha=0.75,
            cmap="viridis",
            label="%d ms IPP" % ipp)

    if scatter is not None:
        cb = fig.colorbar(scatter, ax=ax)
        cb.set_label("SNR (dB)")

    ax.set_xlabel("Time (UTC)")
    ax.set_ylabel("Frequency (MHz)")
    if title is None:
        title = "Digisonde search detections"
    ax.set_title(title)
    ax.grid(True, alpha=0.25)
    ax.legend(loc="best")
    ax.xaxis.set_major_formatter(mdates.DateFormatter("%H:%M"))
    fig.autofmt_xdate()
    if output is None:
        plt.show()
    else:
        fig.savefig(output, dpi=160)
        plt.close(fig)


def main():
    parser = argparse.ArgumentParser(description="Plot digisonde search detections.")
    parser.add_argument(
        "path",
        nargs="?",
        default="/data0",
        help="Day directory, output root, or one digisonde-search HDF5 file")
    parser.add_argument(
        "-o", "--output",
        default=None,
        help="Optional output PNG filename. If omitted, display with plt.show().")
    parser.add_argument("--min-snr", type=float, default=None)
    parser.add_argument("--channel", type=str, default=None)
    parser.add_argument("--ipp-ms", type=int, default=None)
    parser.add_argument("--title", type=str, default=None)
    args = parser.parse_args()

    files = files_from_input(args.path)
    if len(files) == 0:
        print("no digisonde-search files found in %s" % args.path)
        return

    detections = collect_detections(
        files,
        min_snr=args.min_snr,
        channel=args.channel,
        ipp_ms=args.ipp_ms)
    if len(detections) == 0:
        print("no detections left after filtering")
        return

    plot_detections(detections, args.output, title=args.title)
    if args.output is None:
        print("read %d files, showing %d detections" %
              (len(files), len(detections)))
    else:
        print("read %d files, plotted %d detections to %s" %
              (len(files), len(detections), args.output))


if __name__ == "__main__":
    main()
