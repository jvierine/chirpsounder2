#!/usr/bin/env python
#
# Publish a quick-look short-time Fourier transform of a Digital RF channel.
#
import argparse
import configparser
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import tempfile
import time

import digital_rf as drf
import h5py
import matplotlib

matplotlib.use("Agg")

import matplotlib.pyplot as plt
from matplotlib.gridspec import GridSpec
import numpy as n
import scipy.signal as ss

import chirp_config as cc
import chirpsounder_version as csversion


DEFAULT_CONFIG = Path(__file__).resolve().parent / "examples" / "marieluise" / "dombas.ini"
DEFAULT_DATA_DIR = "/dev/shm/hf25"
DEFAULT_CHANNEL = "ch0"
FIGURE_SIZE_INCHES = 16.0
PLOT_DURATION_SECONDS = 60
TIME_RESOLUTION_SECONDS = 0.5


def set_publication_style() -> None:
    plt.rcParams.update(
        {
            "font.size": 13,
            "axes.titlesize": 14,
            "axes.labelsize": 14,
            "xtick.labelsize": 12,
            "ytick.labelsize": 12,
            "legend.fontsize": 12,
            "figure.dpi": 120,
            "savefig.dpi": 300,
        }
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Plot a full-band STFT quick-look from a Digital RF channel."
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
    parser.add_argument("--n-fft", type=int, default=8192, help="FFT length and number of frequency bins.")
    parser.add_argument(
        "--fft-chunk-windows",
        type=int,
        default=128,
        help="Non-overlapping FFT windows to read and transform at once.",
    )
    parser.add_argument(
        "--image-pixels",
        type=int,
        default=8192,
        help="Width and height of the square output PNG in pixels.",
    )
    parser.add_argument(
        "--refresh-sec",
        type=float,
        default=0.0,
        help="Automatically refresh the plot every N seconds. Set <= 0 for a one-shot plot.",
    )
    parser.add_argument(
        "--output",
        default=None,
        help="Output plot filename. Default: /tmp/latest-rf-spectrum-STATION.png",
    )
    parser.add_argument(
        "--store-dir",
        default=None,
        help="HDF5 archive root. Default: the configured output_dir.",
    )
    parser.add_argument(
        "--no-store",
        action="store_true",
        help="Do not write the minute HDF5 product.",
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


def load_frequency_overlay(conf: cc.chirp_config) -> dict | None:
    configured_path = getattr(conf, "rf_spectrum_frequency_overlay_config", None)
    if not configured_path:
        return None

    path = Path(configured_path).expanduser()
    if not path.is_absolute():
        path = Path(conf.fname).resolve().parent / path
    parser = configparser.ConfigParser(interpolation=None)
    if not parser.read(path):
        raise FileNotFoundError(f"RF spectrum frequency overlay config not found: {path}")

    section = parser["config"]
    frequency_rows = json.loads(section["freqs"])
    frequencies_hz = n.asarray([float(row[0]) * 1e6 for row in frequency_rows])
    frequency_duration = float(json.loads(section["frequency_duration"]))
    if frequencies_hz.size == 0 or frequency_duration <= 0:
        raise ValueError(f"invalid frequency schedule in {path}")

    label = json.loads(section.get("instrument_name", '"Ionosonde"'))
    code_types = json.loads(section.get("code_type", "[]"))
    if code_types:
        label = f"{label} ({'/'.join(str(value).upper() for value in code_types)})"

    # This reproduces sweep.determine_sweep_length() in src/ionosonde.
    n_minutes = n.ceil(frequencies_hz.size * frequency_duration / 60.0)
    n_sweeps_per_day = n.floor(24.0 * 60.0 / n_minutes)
    sweep_period = 24.0 * 60.0 * 60.0 / n_sweeps_per_day
    return {
        "path": str(path),
        "label": label,
        "frequencies_hz": frequencies_hz,
        "frequency_duration": frequency_duration,
        "sweep_period": float(sweep_period),
    }


def add_frequency_overlay(data: dict, overlay: dict | None) -> None:
    if overlay is None:
        return
    duration = overlay["frequency_duration"]
    times = n.arange(
        data["minute_start_unix"],
        data["minute_end_unix"] + 0.5 * duration,
        duration,
    )
    phase = n.mod(times, overlay["sweep_period"])
    indices = n.floor(phase / duration).astype(int)
    frequencies = n.full(times.shape, n.nan, dtype=float)
    valid = indices < overlay["frequencies_hz"].size
    frequencies[valid] = overlay["frequencies_hz"][indices[valid]]
    data["overlay_time_unix"] = times
    data["overlay_frequency_hz"] = frequencies
    data["overlay_label"] = overlay["label"]
    data["overlay_config"] = overlay["path"]
    data["overlay_sweep_period"] = overlay["sweep_period"]


def add_lfm_overlays(data: dict, conf: cc.chirp_config) -> None:
    selected = set(getattr(conf, "rf_spectrum_lfm_overlay_transmitters", []))
    if not selected:
        return

    schedules = []
    for group in conf.sounder_timings:
        schedules.extend(group if isinstance(group, list) else [group])

    overlays = []
    minute_start = data["minute_start_unix"]
    minute_end = data["minute_end_unix"]
    for schedule in schedules:
        transmitter = str(schedule.get("transmit_name", "unknown"))
        if transmitter not in selected:
            continue
        rate = float(schedule["chirp-rate"])
        repetition = float(schedule["rep"])
        chirp_epoch = float(schedule["chirpt"])
        min_frequency = float(conf.minimum_analysis_frequency)
        max_frequency = float(conf.maximum_analysis_frequency)
        if rate <= 0 or repetition <= 0 or max_frequency <= min_frequency:
            raise ValueError(f"invalid LFM overlay schedule for {transmitter}")
        sweep_duration = (max_frequency - min_frequency) / rate

        first_start = repetition * n.floor((minute_start - chirp_epoch) / repetition) + chirp_epoch
        times = []
        frequencies = []
        sweep_start = first_start - repetition
        while sweep_start < minute_end:
            segment_start = max(minute_start, sweep_start)
            segment_end = min(minute_end, sweep_start + sweep_duration)
            if segment_end >= segment_start:
                times.extend([segment_start, segment_end, n.nan])
                frequencies.extend(
                    [
                        min_frequency + rate * (segment_start - sweep_start),
                        min_frequency + rate * (segment_end - sweep_start),
                        n.nan,
                    ]
                )
            sweep_start += repetition
        overlays.append(
            {
                "transmitter": transmitter,
                "time_unix": n.asarray(times),
                "frequency_hz": n.asarray(frequencies),
                "chirp_rate_hz_per_s": rate,
                "repetition_s": repetition,
                "chirp_epoch_s": chirp_epoch,
            }
        )
    data["lfm_overlays"] = overlays


def collect_plot_data(reader: drf.DigitalRFReader, conf: cc.chirp_config, args: argparse.Namespace) -> dict:
    channel = conf.channel[0]
    bounds = reader.get_bounds(channel)
    if args.n_fft <= 0 or args.fft_chunk_windows <= 0:
        raise ValueError("n-fft and fft-chunk-windows must be positive")

    n_spec = int(round(PLOT_DURATION_SECONDS / TIME_RESOLUTION_SECONDS))
    n_fft = args.n_fft
    available = int(bounds[1] - bounds[0])
    bin_samples = int(round(TIME_RESOLUTION_SECONDS * conf.sample_rate))
    minute_samples = int(round(PLOT_DURATION_SECONDS * conf.sample_rate))
    required = n_spec * bin_samples
    if bin_samples < n_fft:
        raise ValueError("time resolution is shorter than one FFT")
    if available < required + conf.sample_rate:
        raise RuntimeError(
            f"not enough Digital RF data: need {required + conf.sample_rate} samples, have {available}"
        )

    # Select the newest fully recorded UTC minute. Digital RF sample indices
    # are Unix-epoch based, so integer multiples of 60 * sample_rate are exact
    # wall-clock minute boundaries.
    safe_bound = int(bounds[1] - conf.sample_rate)
    minute_end_sample = (safe_bound // minute_samples) * minute_samples
    i0 = minute_end_sample - minute_samples
    if i0 < int(bounds[0]):
        raise RuntimeError("the complete UTC minute is no longer in the ringbuffer")
    windows_per_bin = bin_samples // n_fft
    wf = n.array(ss.windows.hann(n_fft, sym=False), dtype=n.float32)
    spec = n.zeros([n_fft, n_spec], dtype=n.float32)
    rms_voltage = 0.0
    n_rms_voltage = 0.0
    tvec = n.zeros(n_spec)
    fvec = n.fft.fftshift(n.fft.fftfreq(n_fft, d=1.0 / conf.sample_rate)) + conf.center_freq

    for i in range(n_spec):
        n_accumulated = 0
        rms_power_sum = 0.0
        rms_sample_count = 0
        for first_window in range(0, windows_per_bin, args.fft_chunk_windows):
            chunk_windows = min(args.fft_chunk_windows, windows_per_bin - first_window)
            chunk_samples = chunk_windows * n_fft
            sample = i0 + i * bin_samples + first_window * n_fft
            try:
                if hasattr(reader, "read_vector_1d"):
                    z = reader.read_vector_1d(sample, chunk_samples, channel).astype(
                        "c8", casting="unsafe", copy=False
                    )
                else:  # pragma: no cover - compatibility with Digital RF 2
                    z = reader.read_vector_c81d(sample, chunk_samples, channel)
                z = z.reshape(chunk_windows, n_fft)
                transformed = n.fft.fft(z * wf, axis=1)
                power = transformed.real**2 + transformed.imag**2
                spec[:, i] += n.fft.fftshift(n.sum(power, axis=0))
                rms_power_sum += float(n.sum(z.real**2 + z.imag**2))
                rms_sample_count += z.size
                n_accumulated += chunk_windows
            except Exception as exc:
                print(f"skipping unavailable FFT chunk at sample {sample}: {exc}")

        if n_accumulated:
            spec[:, i] /= n_accumulated
        else:
            spec[:, i] = n.nan
        if rms_sample_count:
            rms_voltage += rms_power_sum / rms_sample_count
            n_rms_voltage += 1.0
        tvec[i] = (i0 + (i + 0.5) * bin_samples) / conf.sample_rate

    # Remove the static bandpass shape by normalizing each frequency bin to
    # its own mean power over the displayed minute.
    frequency_mean_power = n.nanmean(spec, axis=1, keepdims=True)
    with n.errstate(divide="ignore", invalid="ignore"):
        dB = 10.0 * n.log10(spec / frequency_mean_power)

    finite_db = dB[n.isfinite(dB)]
    if finite_db.size:
        q1, q3 = n.percentile(finite_db, [25.0, 75.0])
        iqr = q3 - q1
        if iqr > n.finfo(float).eps:
            vmin = float(q1 - 1.5 * iqr)
            vmax = float(q3 + 1.5 * iqr)
        else:
            center = float(n.median(finite_db))
            vmin, vmax = center - 1.0, center + 1.0
    else:
        vmin, vmax = -1.0, 1.0
    rms_voltage = n.sqrt(rms_voltage / max(n_rms_voltage, 1.0))
    return {
        "tvec": tvec,
        "fvec": fvec,
        "power": spec.T,
        "dB": dB,
        "vmin": vmin,
        "vmax": vmax,
        "rms_voltage": rms_voltage,
        "minute_start_unix": i0 / conf.sample_rate,
        "minute_end_unix": minute_end_sample / conf.sample_rate,
        "windows_per_time_bin": windows_per_bin,
        "samples_per_time_bin": bin_samples,
    }


def draw_plot(
    fig: plt.Figure,
    spec_ax,
    cbar_ax,
    conf: cc.chirp_config,
    args: argparse.Namespace,
    data: dict,
) -> None:
    spec_ax.clear()
    cbar_ax.clear()

    plot_t = [datetime.fromtimestamp(value, timezone.utc) for value in data["tvec"]]
    pcm = spec_ax.pcolormesh(
        plot_t,
        data["fvec"] / 1e6,
        data["dB"],
        vmin=data["vmin"],
        vmax=data["vmax"],
        cmap="plasma",
        shading="auto",
    )
    fig.colorbar(pcm, cax=cbar_ax, label="Power / 60-second frequency mean (dB)")
    minute_start = datetime.fromtimestamp(data["minute_start_unix"], timezone.utc)
    spec_ax.set_title(
        f"{conf.station_name} full-band STFT: {minute_start:%Y-%m-%d %H:%M} UTC\n"
        f"{PLOT_DURATION_SECONDS} seconds at {TIME_RESOLUTION_SECONDS:g}-second resolution; "
        f"$V_{{\\mathrm{{RMS}}}}={data['rms_voltage']:1.6f}$ ADC units"
    )
    spec_ax.set_xlabel("Time (UTC)")
    spec_ax.set_ylabel("Frequency (MHz)")
    spec_ax.set_ylim(
        [
            -conf.sample_rate / 2.0 / 1e6 + conf.center_freq / 1e6,
            conf.sample_rate / 2.0 / 1e6 + conf.center_freq / 1e6,
        ]
    )
    spec_ax.set_xlim(
        datetime.fromtimestamp(data["minute_start_unix"], timezone.utc),
        datetime.fromtimestamp(data["minute_end_unix"], timezone.utc),
    )
    overlay_half_width_hz = conf.rf_spectrum_overlay_half_width_hz
    if "overlay_time_unix" in data:
        overlay_times = [
            datetime.fromtimestamp(value, timezone.utc)
            for value in data["overlay_time_unix"]
        ]
        for offset_hz in (-overlay_half_width_hz, overlay_half_width_hz):
            spec_ax.step(
                overlay_times,
                (data["overlay_frequency_hz"] + offset_hz) / 1e6,
                where="post",
                color="cyan",
                linestyle="--",
                linewidth=0.7,
                alpha=0.4,
                label=(
                    f"{data['overlay_label']} ±{overlay_half_width_hz / 1e3:g} kHz"
                    if offset_hz < 0
                    else None
                ),
                zorder=5,
            )
    for overlay in data.get("lfm_overlays", []):
        # Plot each segment separately so datetime conversion never sees the
        # NaN separators retained in the stored overlay arrays.
        for segment_index in range(0, overlay["time_unix"].size, 3):
            segment_times = overlay["time_unix"][segment_index : segment_index + 2]
            segment_frequencies = overlay["frequency_hz"][segment_index : segment_index + 2]
            if segment_times.size != 2 or not n.all(n.isfinite(segment_times)):
                continue
            for offset_hz in (-overlay_half_width_hz, overlay_half_width_hz):
                spec_ax.plot(
                    [datetime.fromtimestamp(value, timezone.utc) for value in segment_times],
                    (segment_frequencies + offset_hz) / 1e6,
                    color="lime",
                    linestyle=":",
                    linewidth=0.8,
                    alpha=0.4,
                    label=(
                        f"{overlay['transmitter']} FMCW ±{overlay_half_width_hz / 1e3:g} kHz"
                        if segment_index == 0 and offset_hz < 0
                        else None
                    ),
                    zorder=5,
                )
    if "overlay_time_unix" in data or data.get("lfm_overlays"):
        spec_ax.legend(loc="upper left", framealpha=0.55)


def save_power_hdf5(conf: cc.chirp_config, args: argparse.Namespace, data: dict) -> Path:
    minute_start = int(round(data["minute_start_unix"]))
    day = datetime.fromtimestamp(minute_start, timezone.utc).strftime("%Y-%m-%d")
    archive_root = Path(args.store_dir or conf.output_dir)
    output_dir = archive_root / day
    output_dir.mkdir(parents=True, exist_ok=True)
    output_path = output_dir / f"rf_spectrum-{conf.station_name}-{conf.channel[0]}-{minute_start}.h5"

    with tempfile.NamedTemporaryFile(
        prefix=f".{output_path.stem}-",
        suffix=output_path.suffix,
        dir=output_dir,
        delete=False,
    ) as temp_file:
        temp_name = temp_file.name
    try:
        with h5py.File(temp_name, "w") as handle:
            csversion.tag_hdf5(handle)
            handle.attrs["schema"] = "chirpsounder2.rf_spectrum.v1"
            handle.attrs["station"] = conf.station_name
            handle.attrs["channel"] = conf.channel[0]
            handle.attrs["window"] = "periodic_hann"
            handle.attrs["fft_length"] = args.n_fft
            handle.attrs["window_overlap_samples"] = 0
            handle.attrs["windows_per_time_bin"] = data["windows_per_time_bin"]
            handle.attrs["samples_per_time_bin"] = data["samples_per_time_bin"]
            handle.attrs["sample_rate_hz"] = conf.sample_rate
            handle.attrs["center_frequency_hz"] = conf.center_freq
            handle.attrs["overlay_half_width_hz"] = conf.rf_spectrum_overlay_half_width_hz
            handle.attrs["power_units"] = "ADC units squared"
            handle.attrs["power_dimensions"] = "time,frequency"
            handle.create_dataset("time_unix", data=data["tvec"], dtype="f8")
            handle.create_dataset("frequency_hz", data=data["fvec"], dtype="f8")
            handle.create_dataset(
                "power",
                data=data["power"],
                dtype="f4",
                chunks=(1, args.n_fft),
                compression="gzip",
                compression_opts=4,
                shuffle=True,
            )
            handle["minute_start_unix"] = data["minute_start_unix"]
            handle["minute_end_unix"] = data["minute_end_unix"]
            if "overlay_time_unix" in data:
                handle.create_dataset("overlay_time_unix", data=data["overlay_time_unix"], dtype="f8")
                handle.create_dataset(
                    "overlay_frequency_hz", data=data["overlay_frequency_hz"], dtype="f8"
                )
                handle.attrs["overlay_label"] = data["overlay_label"]
                handle.attrs["overlay_config"] = data["overlay_config"]
                handle.attrs["overlay_sweep_period_s"] = data["overlay_sweep_period"]
            if data.get("lfm_overlays"):
                overlay_group = handle.create_group("lfm_overlays")
                for overlay in data["lfm_overlays"]:
                    transmitter_group = overlay_group.create_group(overlay["transmitter"])
                    transmitter_group.create_dataset("time_unix", data=overlay["time_unix"])
                    transmitter_group.create_dataset(
                        "frequency_hz", data=overlay["frequency_hz"]
                    )
                    transmitter_group.attrs["chirp_rate_hz_per_s"] = overlay[
                        "chirp_rate_hz_per_s"
                    ]
                    transmitter_group.attrs["repetition_s"] = overlay["repetition_s"]
                    transmitter_group.attrs["chirp_epoch_s"] = overlay["chirp_epoch_s"]
        os.replace(temp_name, output_path)
    finally:
        if os.path.exists(temp_name):
            os.unlink(temp_name)
    print(f"saved {output_path}")
    return output_path


def save_plot(fig: plt.Figure, output: str, dpi: float) -> None:
    output_path = Path(output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile(
        prefix=f".{output_path.stem}-",
        suffix=output_path.suffix,
        dir=output_path.parent,
        delete=False,
    ) as temp_file:
        temp_name = temp_file.name
    try:
        # Avoid bbox_inches="tight": a fixed canvas and DPI guarantee exact dimensions.
        fig.savefig(temp_name, dpi=dpi)
        os.replace(temp_name, output_path)
    finally:
        if os.path.exists(temp_name):
            os.unlink(temp_name)
    print(f"saved {output}")


def main() -> None:
    args = parse_args()
    set_publication_style()
    conf = load_config(args)
    if args.image_pixels <= 0:
        raise ValueError("image-pixels must be positive")
    output = args.output or f"/tmp/latest-rf-spectrum-{conf.station_name}.png"
    reader = drf.DigitalRFReader(conf.data_dir)
    frequency_overlay = load_frequency_overlay(conf)

    output_dpi = args.image_pixels / FIGURE_SIZE_INCHES
    fig = plt.figure(figsize=(FIGURE_SIZE_INCHES, FIGURE_SIZE_INCHES), dpi=output_dpi)
    gs = GridSpec(1, 2, figure=fig, width_ratios=[30, 1])
    spec_ax = fig.add_subplot(gs[0, 0])
    cbar_ax = fig.add_subplot(gs[0, 1])
    fig.subplots_adjust(left=0.08, right=0.91, bottom=0.07, top=0.94, wspace=0.08)

    while True:
        try:
            data = collect_plot_data(reader, conf, args)
        except RuntimeError as exc:
            if args.refresh_sec <= 0:
                raise
            # With a 70-second ringbuffer, a process started late in a minute
            # cannot recover that minute's beginning. Skip it and start just
            # after the next UTC boundary instead of storing a partial minute.
            wait_seconds = 61.0 - (time.time() % 60.0)
            print(f"{exc}; waiting {wait_seconds:.1f} seconds for the next full UTC minute")
            time.sleep(max(0.1, wait_seconds))
            continue
        add_frequency_overlay(data, frequency_overlay)
        add_lfm_overlays(data, conf)
        if not args.no_store:
            save_power_hdf5(conf, args, data)
        draw_plot(fig, spec_ax, cbar_ax, conf, args, data)
        save_plot(fig, output, output_dpi)
        if args.refresh_sec <= 0:
            break
        # Begin the next product one second after its UTC minute closes. If the
        # preceding cycle ran long, start immediately and let collect_plot_data
        # skip a minute whose beginning has already left the ringbuffer.
        next_minute_ready = data["minute_end_unix"] + PLOT_DURATION_SECONDS + 1.0
        time.sleep(max(0.0, next_minute_ready - time.time()))


if __name__ == "__main__":
    main()
