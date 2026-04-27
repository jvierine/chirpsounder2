#!/usr/bin/env python
#
# Plot a quick overview of the time-frequency spectrum together with
# a short segment of raw complex RF.
#
import argparse
from pathlib import Path

import digital_rf as drf
import matplotlib.pyplot as plt
import numpy as n
import scipy.signal as ss

import chirp_config as cc
import chirp_det as cd


DEFAULT_CONFIG = Path(__file__).resolve().parent / "examples" / "marieluise" / "dombas.ini"
DEFAULT_DATA_DIR = "/dev/shm/hf25"
DEFAULT_CHANNEL = "ch0"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Plot a spectrogram overview and 100 ms of raw RF from a DigitalRF channel."
    )
    parser.add_argument(
        "config",
        nargs="?",
        default=str(DEFAULT_CONFIG),
        help=f"Configuration file to load. Default: {DEFAULT_CONFIG}",
    )
    parser.add_argument(
        "--data-dir",
        default=None,
        help=f"Override the DigitalRF data directory. Default without config data_dir: {DEFAULT_DATA_DIR}",
    )
    parser.add_argument(
        "--channel",
        default=None,
        help=f"Override the channel name. Default without config channel: {DEFAULT_CHANNEL}",
    )
    parser.add_argument("--n-spec", type=int, default=100, help="Number of spectrogram columns to plot.")
    parser.add_argument("--n-avg", type=int, default=10, help="Number of FFTs to average per spectrogram column.")
    parser.add_argument("--n-fft", type=int, default=4096, help="FFT length.")
    parser.add_argument(
        "--raw-duration-ms",
        type=float,
        default=100.0,
        help="Length of raw RF to plot in milliseconds.",
    )
    parser.add_argument(
        "--refresh-sec",
        type=float,
        default=0.0,
        help="Automatically refresh the plot every N seconds. Set <= 0 for a one-shot plot.",
    )
    return parser.parse_args()


def load_config(args: argparse.Namespace) -> cc.chirp_config:
    conf = cc.chirp_config(args.config)
    if args.data_dir is not None:
        conf.data_dir = args.data_dir
    elif not getattr(conf, "data_dir", None):
        conf.data_dir = DEFAULT_DATA_DIR

    if args.channel is not None:
        conf.channel = [args.channel]
    elif not getattr(conf, "channel", None):
        conf.channel = [DEFAULT_CHANNEL]

    return conf


def collect_plot_data(reader: drf.DigitalRFReader, conf: cc.chirp_config, args: argparse.Namespace) -> dict:
    channel = conf.channel[0]
    bounds = reader.get_bounds(channel)
    n_spec = args.n_spec
    n_avg = args.n_avg
    n_fft = args.n_fft
    raw_n_samples = max(1, int(round(conf.sample_rate * args.raw_duration_ms / 1000.0)))

    dt = max(1, int(n.floor((bounds[1] - bounds[0] - conf.sample_rate) / n_spec)))
    wf = n.array(ss.windows.hann(n_fft), dtype=n.float32)
    spec = n.zeros([n_fft, n_spec], dtype=n.float32)
    latest_span = (n_spec - 1) * dt + n_avg * n_fft
    i0 = max(int(bounds[0] + conf.sample_rate), int(bounds[1] - conf.sample_rate - latest_span))
    rms_voltage = 0.0
    n_rms_voltage = 0.0
    tvec = n.zeros(n_spec)
    fvec = n.fft.fftshift(n.fft.fftfreq(n_fft, d=1.0 / conf.sample_rate)) / 1e6 + conf.center_freq / 1e6

    raw_i0 = i0
    raw_z = None
    try:
        raw_z = reader.read_vector_c81d(raw_i0, raw_n_samples, channel)
    except Exception:
        print("missing raw RF segment")

    for i in range(n_spec):
        print(i)
        for j in range(n_avg):
            try:
                z = reader.read_vector_c81d(i0 + i * dt + j * n_fft, n_fft, channel)
                rms_voltage += n.mean(n.abs(z) ** 2.0)
                n_rms_voltage += 1.0
                spec[:, i] += n.fft.fftshift(n.abs(cd.fft(wf * z)) ** 2.0)
            except Exception:
                print("missing data")

        tvec[i] = i * dt / conf.sample_rate

    dB = 10.0 * n.log10(n.maximum(spec, 1e-12))
    dB = dB - n.nanmedian(dB)
    rms_voltage = n.sqrt(rms_voltage / max(n_rms_voltage, 1.0))
    raw_t = None if raw_z is None else n.arange(raw_z.size) / conf.sample_rate * 1e3

    return {
        "tvec": tvec,
        "fvec": fvec,
        "dB": dB,
        "rms_voltage": rms_voltage,
        "raw_t": raw_t,
        "raw_z": raw_z,
    }


def draw_plot(fig: plt.Figure, axes, conf: cc.chirp_config, args: argparse.Namespace, data: dict) -> None:
    axes[0].clear()
    axes[1].clear()
    for extra_ax in fig.axes[2:]:
        extra_ax.remove()

    pcm = axes[0].pcolormesh(data["tvec"], data["fvec"], data["dB"], vmin=-10, vmax=50.0, cmap="plasma", shading="auto")
    fig.colorbar(pcm, ax=axes[0], label="Relative power (dB)")
    axes[0].set_title(f"$V_{{\\mathrm{{RMS}}}}={data['rms_voltage']:1.6f}$ (ADC units)")
    axes[0].set_xlabel("Time (s)")
    axes[0].set_ylabel("Frequency (MHz)")
    axes[0].set_ylim(
        [
            -conf.sample_rate / 2.0 / 1e6 + conf.center_freq / 1e6,
            conf.sample_rate / 2.0 / 1e6 + conf.center_freq / 1e6,
        ]
    )

    if data["raw_z"] is not None:
        axes[1].plot(data["raw_t"], data["raw_z"].real, label="Re", linewidth=0.8)
        axes[1].plot(data["raw_t"], data["raw_z"].imag, label="Im", linewidth=0.8)
        axes[1].set_title(f"Raw RF, {args.raw_duration_ms:g} ms")
        axes[1].set_xlabel("Time (ms)")
        axes[1].set_ylabel("ADC units")
        axes[1].legend(loc="upper right")
    else:
        axes[1].text(0.5, 0.5, "Raw RF unavailable", ha="center", va="center", transform=axes[1].transAxes)
        axes[1].set_title(f"Raw RF, {args.raw_duration_ms:g} ms")
        axes[1].set_xlabel("Time (ms)")
        axes[1].set_ylabel("ADC units")

    fig.canvas.draw_idle()


def main() -> None:
    args = parse_args()
    conf = load_config(args)
    reader = drf.DigitalRFReader(conf.data_dir)

    fig, axes = plt.subplots(2, 1, figsize=(11, 8), constrained_layout=True)

    if args.refresh_sec > 0:
        plt.ion()
        while plt.fignum_exists(fig.number):
            data = collect_plot_data(reader, conf, args)
            draw_plot(fig, axes, conf, args, data)
            plt.pause(args.refresh_sec)
    else:
        data = collect_plot_data(reader, conf, args)
        draw_plot(fig, axes, conf, args, data)
        plt.show()


if __name__ == "__main__":
    main()
