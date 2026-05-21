#!/usr/bin/env python3
"""Locate candidate chirp transmitters from multi-station receive times.

The consolidated detection files contain rows with

    chirp_time, sample_time, f0, chirp_rate, snr

For this script, ``chirp_time`` is treated as the observed virtual receive time
of a chirp at one receiver. Detections are grouped only by
``floor(chirp_time)``. If the same integer chirp time is observed at KHO, TGO,
and Dombas, all detections in that group are treated as independent receive-time
measurements of the same sounder.

For each group, the fitted model is

    chirp_time_i = transmit_time + great_circle_range(tx, rx_i)/(b*c),

where ``tx`` is the unknown transmitter latitude/longitude, ``rx_i`` is the
known receiver location, ``c`` is the speed of light in vacuum, and ``b`` is an
effective ionospheric group-velocity factor. The default is b=0.90, but this is
only a first-order fudge factor for ionospheric group delay, ray path curvature,
and reflection altitude.
"""

from __future__ import annotations

import argparse
import ast
import configparser
import csv
import glob
import os
import re
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path

import h5py
import matplotlib.pyplot as plt
import numpy as np
from scipy.optimize import least_squares

try:
    import cartopy.crs as ccrs
    import cartopy.feature as cfeature
except Exception:
    ccrs = None
    cfeature = None


C_KM_S = 299792.458
EARTH_RADIUS_KM = 6371.0


@dataclass
class DetectionObservation:
    station: str
    chirp_time_s: float
    frequency_hz: float
    chirp_rate_hz_s: float
    snr: float


@dataclass
class LocatedEvent:
    chirp_second: int
    transmit_time_s: float
    lat_deg: float
    lon_deg: float
    median_chirp_rate_hz_s: float
    n_observations: int
    rms_residual_ms: float
    max_abs_residual_ms: float
    rms_residual_km: float
    station_counts: dict[str, int]
    observations: list[DetectionObservation]
    residuals_ms: list[float]


@dataclass
class GroupStats:
    chirp_second: int
    stations: tuple[str, ...]
    median_chirp_rate_hz_s: float
    n_observations: int
    time_span_ms: float
    station_counts: dict[str, int]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Locate chirp transmitters from KHO/TGO/Dombas receive-time detections."
    )
    parser.add_argument(
        "--data-dir",
        default="/data1/oblique_ionosonde",
        help="Directory containing YYYY-MM-DD/cdetections-*.h5 files.",
    )
    parser.add_argument("--date", default="2026-05-19", help="UTC date to analyze.")
    parser.add_argument(
        "--config",
        default="examples/marieluise/server.ini",
        help="Configuration file containing [stations].station_info.",
    )
    parser.add_argument(
        "--stations",
        default="KHO,TGO,DOB",
        help="Comma-separated receiver station codes required for localization. At least three are required.",
    )
    parser.add_argument(
        "--group-velocity-frac",
        type=float,
        default=0.90,
        help="Effective ionospheric group velocity as a fraction of c.",
    )
    parser.add_argument(
        "--max-group-span-s",
        type=float,
        default=0.25,
        help="Reject groups where chirp_time span across all observations is larger than this.",
    )
    parser.add_argument(
        "--min-snr",
        type=float,
        default=0.0,
        help="Minimum SNR for individual detections before grouping.",
    )
    parser.add_argument(
        "--max-rms-residual-ms",
        type=float,
        default=10.0,
        help="Maximum RMS timing residual retained in the output table and plots.",
    )
    parser.add_argument(
        "--max-source-distance-km",
        type=float,
        default=22000.0,
        help="Reject fitted source locations farther than this from any required station.",
    )
    parser.add_argument(
        "--output-dir",
        default="/tmp",
        help="Directory where CSV and plot files are written.",
    )
    parser.add_argument(
        "--event-plot-limit",
        type=int,
        default=0,
        help="Maximum number of individual event plots to write. Use 0 for all retained events.",
    )
    return parser.parse_args()


def load_station_info(config_path: str) -> dict[str, dict[str, float]]:
    config = configparser.ConfigParser(interpolation=None)
    config.read(config_path)
    return ast.literal_eval(config["stations"]["station_info"])


def station_from_filename(path: str) -> str | None:
    match = re.search(r"cdetections-([A-Z0-9]+)-[0-9]+\.h5$", os.path.basename(path))
    if match is None:
        return None
    return match.group(1)


def read_grouped_observations(
    data_dir: str,
    date: str,
    stations: list[str],
    min_snr: float,
) -> dict[int, dict[str, list[DetectionObservation]]]:
    """Group observations only by floor(chirp_time)."""
    required = set(stations)
    groups: dict[int, dict[str, list[DetectionObservation]]] = {}
    pattern = str(Path(data_dir) / date / "cdetections-*.h5")
    for fname in sorted(glob.glob(pattern)):
        station = station_from_filename(fname)
        if station not in required:
            continue
        with h5py.File(fname, "r") as h:
            data = h["data"][:]
        if data.size == 0:
            continue
        if min_snr > 0:
            data = data[data[:, 4] >= min_snr]
            if data.size == 0:
                continue
        seconds = np.floor(data[:, 0]).astype(np.int64)
        for sec, chirp_time, f0, chirp_rate, snr in zip(
            seconds, data[:, 0], data[:, 2], data[:, 3], data[:, 4]
        ):
            groups.setdefault(int(sec), {}).setdefault(station, []).append(
                DetectionObservation(
                    station=station,
                    chirp_time_s=float(chirp_time),
                    frequency_hz=float(f0),
                    chirp_rate_hz_s=float(chirp_rate),
                    snr=float(snr),
                )
            )
    return groups


def group_statistics(
    groups: dict[int, dict[str, list[DetectionObservation]]],
    stations: list[str],
) -> list[GroupStats]:
    stats: list[GroupStats] = []
    requested = set(stations)
    for chirp_second, by_station in sorted(groups.items()):
        rate_values = sorted(
            {
                int(round(obs.chirp_rate_hz_s / 1e3))
                for station_observations in by_station.values()
                for obs in station_observations
            }
        )
        for rate_khz in rate_values:
            present = []
            rate_observations = []
            for station in stations:
                station_observations = [
                    obs
                    for obs in by_station.get(station, [])
                    if int(round(obs.chirp_rate_hz_s / 1e3)) == rate_khz
                ]
                if station_observations:
                    present.append(station)
                    rate_observations.extend(station_observations)
            if len(present) < 2:
                continue
            times = np.array([obs.chirp_time_s for obs in rate_observations], dtype=float)
            stats.append(
                GroupStats(
                    chirp_second=chirp_second,
                    stations=tuple(present),
                    median_chirp_rate_hz_s=rate_khz * 1e3,
                    n_observations=len(rate_observations),
                    time_span_ms=float(np.ptp(times) * 1e3),
                    station_counts={
                        station: sum(
                            int(round(obs.chirp_rate_hz_s / 1e3)) == rate_khz
                            for obs in by_station.get(station, [])
                        )
                        for station in requested
                    },
                )
            )
    return stats


def great_circle_km(lat1_deg: float, lon1_deg: float, lat2_deg, lon2_deg) -> np.ndarray:
    lat1 = np.radians(lat1_deg)
    lon1 = np.radians(lon1_deg)
    lat2 = np.radians(lat2_deg)
    lon2 = np.radians(lon2_deg)
    dlat = lat2 - lat1
    dlon = lon2 - lon1
    a = np.sin(dlat / 2.0) ** 2 + np.cos(lat1) * np.cos(lat2) * np.sin(dlon / 2.0) ** 2
    return EARTH_RADIUS_KM * 2.0 * np.arcsin(np.sqrt(np.clip(a, 0.0, 1.0)))


def observations_to_arrays(
    observations: list[DetectionObservation],
    station_info: dict[str, dict[str, float]],
) -> tuple[np.ndarray, np.ndarray, np.ndarray, np.ndarray, np.ndarray, list[str]]:
    times = np.array([o.chirp_time_s for o in observations], dtype=float)
    lats = np.array([station_info[o.station]["lat"] for o in observations], dtype=float)
    lons = np.array([station_info[o.station]["lon"] for o in observations], dtype=float)
    freqs = np.array([o.frequency_hz for o in observations], dtype=float)
    rates = np.array([o.chirp_rate_hz_s for o in observations], dtype=float)
    stations = [o.station for o in observations]
    return times, lats, lons, freqs, rates, stations


def best_tx_time_for_position(
    lat_deg: float,
    lon_deg: float,
    rx_lats: np.ndarray,
    rx_lons: np.ndarray,
    obs_times: np.ndarray,
    velocity_km_s: float,
) -> float:
    ranges = great_circle_km(lat_deg, lon_deg, rx_lats, rx_lons)
    # The median is robust to occasional false detections in a same-second group.
    return float(np.median(obs_times - ranges / velocity_km_s))


def initial_guess(
    rx_lats: np.ndarray,
    rx_lons: np.ndarray,
    obs_times: np.ndarray,
    velocity_km_s: float,
) -> tuple[float, float, float]:
    """Coarse global grid search for a stable nonlinear least-squares seed."""
    lat_grid = np.linspace(-85.0, 85.0, 86)
    lon_grid = np.linspace(-180.0, 180.0, 181)
    lon_mesh, lat_mesh = np.meshgrid(lon_grid, lat_grid)
    test_lats = lat_mesh.ravel()
    test_lons = lon_mesh.ravel()

    ranges = great_circle_km(
        test_lats[:, None],
        test_lons[:, None],
        rx_lats[None, :],
        rx_lons[None, :],
    )
    tx_times = np.median(obs_times[None, :] - ranges / velocity_km_s, axis=1)
    residual_s = tx_times[:, None] + ranges / velocity_km_s - obs_times[None, :]
    cost = np.median(np.abs(residual_s), axis=1)
    best_idx = int(np.argmin(cost))
    return float(test_lats[best_idx]), float(test_lons[best_idx]), float(tx_times[best_idx])


def solve_event(
    observations: list[DetectionObservation],
    station_info: dict[str, dict[str, float]],
    velocity_km_s: float,
) -> tuple[float, float, float, np.ndarray]:
    obs_times, rx_lats, rx_lons, _, _, obs_stations = observations_to_arrays(observations, station_info)
    t_ref = float(np.floor(np.median(obs_times)))

    # Use one robust receive-time estimate per station for the expensive global
    # seed search. The final least-squares fit below still uses every detection.
    seed_lats = []
    seed_lons = []
    seed_times = []
    for station in sorted(set(obs_stations)):
        idx = np.array([s == station for s in obs_stations], dtype=bool)
        seed_lats.append(station_info[station]["lat"])
        seed_lons.append(station_info[station]["lon"])
        seed_times.append(float(np.median(obs_times[idx])))
    lat0, lon0, tx0 = initial_guess(
        np.array(seed_lats, dtype=float),
        np.array(seed_lons, dtype=float),
        np.array(seed_times, dtype=float),
        velocity_km_s,
    )

    def residual(x: np.ndarray) -> np.ndarray:
        lat, lon, tx_offset = x
        ranges = great_circle_km(lat, lon, rx_lats, rx_lons)
        predicted = t_ref + tx_offset + ranges / velocity_km_s
        return (predicted - obs_times) * velocity_km_s

    fit = least_squares(
        residual,
        x0=np.array([lat0, lon0, tx0 - t_ref]),
        bounds=([-89.0, -180.0, -2.0], [89.0, 180.0, 2.0]),
        loss="soft_l1",
        f_scale=velocity_km_s * 0.01,
        xtol=1e-10,
        ftol=1e-10,
        gtol=1e-10,
        max_nfev=500,
    )
    lat, lon, tx_offset = fit.x
    residual_km = residual(fit.x)
    return float(lat), float(lon), float(t_ref + tx_offset), residual_km


def locate_events(
    groups: dict[int, dict[str, list[DetectionObservation]]],
    stations: list[str],
    station_info: dict[str, dict[str, float]],
    velocity_km_s: float,
    max_group_span_s: float,
    max_rms_residual_ms: float,
    max_source_distance_km: float,
) -> list[LocatedEvent]:
    required = set(stations)
    events: list[LocatedEvent] = []
    for chirp_second, by_station in sorted(groups.items()):
        # Only localize events detected at every required receiver. With the
        # default configuration this means a true three-station KHO+TGO+DOB
        # same-floor(chirp_time) detection.
        if set(by_station.keys()) != required:
            continue
        observations = [obs for station in stations for obs in by_station[station]]
        obs_times = np.array([o.chirp_time_s for o in observations], dtype=float)
        if float(np.ptp(obs_times)) > max_group_span_s:
            continue

        lat, lon, tx_time, residual_km = solve_event(observations, station_info, velocity_km_s)
        station_ranges = np.array(
            [
                great_circle_km(lat, lon, station_info[station]["lat"], station_info[station]["lon"])
                for station in stations
            ],
            dtype=float,
        )
        if np.max(station_ranges) > max_source_distance_km:
            continue

        residual_ms = residual_km / velocity_km_s * 1e3
        rms_ms = float(np.sqrt(np.mean(residual_ms**2)))
        if rms_ms > max_rms_residual_ms:
            continue

        station_counts = {station: len(by_station[station]) for station in stations}
        events.append(
            LocatedEvent(
                chirp_second=chirp_second,
                transmit_time_s=tx_time,
                lat_deg=lat,
                lon_deg=lon,
                median_chirp_rate_hz_s=float(np.median([o.chirp_rate_hz_s for o in observations])),
                n_observations=len(observations),
                rms_residual_ms=rms_ms,
                max_abs_residual_ms=float(np.max(np.abs(residual_ms))),
                rms_residual_km=float(np.sqrt(np.mean(residual_km**2))),
                station_counts=station_counts,
                observations=observations,
                residuals_ms=list(residual_ms),
            )
        )
    return events


def write_csv(events: list[LocatedEvent], ofname: Path, stations: list[str]) -> None:
    with ofname.open("w", newline="") as fh:
        writer = csv.writer(fh)
        header = [
            "chirp_second",
            "chirp_second_utc",
            "transmit_time_s",
            "transmit_time_utc",
            "lat_deg",
            "lon_deg",
            "median_chirp_rate_hz_s",
            "n_observations",
            "rms_residual_ms",
            "max_abs_residual_ms",
            "rms_residual_km",
        ]
        header += [f"{station}_count" for station in stations]
        writer.writerow(header)
        for event in events:
            row = [
                event.chirp_second,
                datetime.fromtimestamp(event.chirp_second, timezone.utc).isoformat(),
                event.transmit_time_s,
                datetime.fromtimestamp(event.transmit_time_s, timezone.utc).isoformat(),
                event.lat_deg,
                event.lon_deg,
                event.median_chirp_rate_hz_s,
                event.n_observations,
                event.rms_residual_ms,
                event.max_abs_residual_ms,
                event.rms_residual_km,
            ]
            row += [event.station_counts.get(station, 0) for station in stations]
            writer.writerow(row)


def plot_events(
    events: list[LocatedEvent],
    station_info: dict[str, dict[str, float]],
    stations: list[str],
    output_dir: Path,
    date: str,
) -> None:
    if not events:
        print("No events to plot")
        return

    lats = np.array([e.lat_deg for e in events])
    lons = np.array([e.lon_deg for e in events])
    rms_ms = np.array([e.rms_residual_ms for e in events])
    nobs = np.array([e.n_observations for e in events])
    times = np.array([e.chirp_second for e in events])
    hours = (times - times.min()) / 3600.0

    plt.rcParams.update(
        {
            "font.size": 13,
            "axes.labelsize": 14,
            "axes.titlesize": 15,
            "legend.fontsize": 12,
        }
    )

    fig, ax = plt.subplots(figsize=(10, 8), constrained_layout=True)
    sc = ax.scatter(lons, lats, c=hours, s=np.clip(nobs, 20, 180), cmap="viridis",
                    edgecolor="k", linewidth=0.25, alpha=0.9)
    for station in stations:
        st = station_info[station]
        ax.scatter(st["lon"], st["lat"], marker="^", s=130, color="tab:red", edgecolor="k")
        ax.text(st["lon"] + 0.6, st["lat"] + 0.4, station, weight="bold")
    ax.set_xlabel("Longitude (deg)")
    ax.set_ylabel("Latitude (deg)")
    ax.set_title(f"Candidate chirp-source positions from three-station receive-time fits, {date}")
    ax.grid(alpha=0.3)
    pad = 8.0
    ax.set_xlim(min(lons.min(), *(station_info[s]["lon"] for s in stations)) - pad,
                max(lons.max(), *(station_info[s]["lon"] for s in stations)) + pad)
    ax.set_ylim(max(-89.0, min(lats.min(), *(station_info[s]["lat"] for s in stations)) - pad),
                min(89.0, max(lats.max(), *(station_info[s]["lat"] for s in stations)) + pad))
    cb = fig.colorbar(sc, ax=ax)
    cb.set_label("Hours since first retained event")
    map_file = output_dir / f"chirp_source_locations_{date}.png"
    fig.savefig(map_file, dpi=200)
    plt.close(fig)
    print(f"wrote {map_file}")

    if ccrs is not None:
        fig = plt.figure(figsize=(13, 7), constrained_layout=True)
        projection = ccrs.PlateCarree()
        ax_map = fig.add_subplot(1, 1, 1, projection=projection)
        ax_map.set_global()
        ax_map.add_feature(cfeature.LAND, facecolor="0.92", edgecolor="none")
        ax_map.add_feature(cfeature.OCEAN, facecolor="0.98", edgecolor="none")
        ax_map.coastlines(linewidth=0.8, color="0.35")
        ax_map.add_feature(cfeature.BORDERS, linewidth=0.35, edgecolor="0.55")
        gl = ax_map.gridlines(
            draw_labels=True,
            linewidth=0.35,
            color="0.55",
            alpha=0.6,
            linestyle="--",
        )
        gl.top_labels = False
        gl.right_labels = False

        sc = ax_map.scatter(
            lons,
            lats,
            c=hours,
            s=np.clip(nobs, 20, 180),
            cmap="viridis",
            edgecolor="k",
            linewidth=0.25,
            alpha=0.9,
            transform=projection,
            zorder=4,
        )
        for station in stations:
            st = station_info[station]
            ax_map.scatter(
                st["lon"],
                st["lat"],
                marker="^",
                s=130,
                color="tab:red",
                edgecolor="k",
                transform=projection,
                zorder=5,
            )
            ax_map.text(
                st["lon"] + 2.0,
                st["lat"] + 2.0,
                station,
                weight="bold",
                transform=projection,
                zorder=6,
            )
        ax_map.set_title(f"Global candidate chirp-source positions, {date}")
        cb = fig.colorbar(sc, ax=ax_map, shrink=0.78, pad=0.03)
        cb.set_label("Hours since first retained event")
        global_map_file = output_dir / f"chirp_source_locations_global_{date}.png"
        fig.savefig(global_map_file, dpi=220)
        plt.close(fig)
        print(f"wrote {global_map_file}")
    else:
        print("Cartopy is not available; skipped global map")

    fig, ax = plt.subplots(2, 1, figsize=(10, 7), constrained_layout=True)
    ax[0].hist(rms_ms, bins=30, color="0.25")
    ax[0].set_xlabel("RMS timing residual (ms)")
    ax[0].set_ylabel("Events")
    ax[0].set_title("Receive-time fit residuals")
    ax[1].scatter(nobs, rms_ms, c=hours, s=32, cmap="viridis", edgecolor="none")
    ax[1].set_xlabel("Number of detections in same-second group")
    ax[1].set_ylabel("RMS timing residual (ms)")
    ax[1].grid(alpha=0.3)
    residual_file = output_dir / f"chirp_source_residuals_{date}.png"
    fig.savefig(residual_file, dpi=200)
    plt.close(fig)

    print(f"wrote {residual_file}")


def plot_individual_events(
    events: list[LocatedEvent],
    stations: list[str],
    output_dir: Path,
    date: str,
    limit: int,
) -> None:
    if not events:
        print("No individual event plots to write")
        return

    event_dir = output_dir / f"chirp_event_plots_{date}"
    event_dir.mkdir(parents=True, exist_ok=True)
    events_to_plot = events if limit == 0 else events[:limit]
    colors = {
        "KHO": "#E45756",
        "TGO": "#4C78A8",
        "DOB": "#F58518",
        "DMB": "#72B7B2",
    }

    for event in events_to_plot:
        fig, ax = plt.subplots(figsize=(9, 5.6), constrained_layout=True)
        all_times = np.array([obs.chirp_time_s for obs in event.observations], dtype=float)
        t0 = float(np.floor(np.median(all_times)))

        for station in stations:
            obs = [o for o in event.observations if o.station == station]
            if not obs:
                continue
            freq_mhz = np.array([o.frequency_hz / 1e6 for o in obs], dtype=float)
            chirp_ms = np.array([(o.chirp_time_s - t0) * 1e3 for o in obs], dtype=float)
            snr = np.array([o.snr for o in obs], dtype=float)
            sizes = np.clip(10.0 + 1.8 * snr, 18.0, 130.0)
            ax.scatter(
                freq_mhz,
                chirp_ms,
                s=sizes,
                color=colors.get(station),
                alpha=0.72,
                edgecolor="none",
                label=f"{station} ({len(obs)})",
            )

            # Show a robust station-level timing reference as a horizontal line.
            ax.axhline(
                np.median(chirp_ms),
                color=colors.get(station),
                linewidth=1.6,
                alpha=0.85,
            )

        residual_ms = np.array(event.residuals_ms, dtype=float)
        ax.set_xlabel("Detected frequency (MHz)")
        ax.set_ylabel(f"Chirp time - {int(t0)} (ms)")
        ax.set_title(
            "Three-station chirp-time detections\n"
            f"{datetime.fromtimestamp(event.chirp_second, timezone.utc).isoformat()}  "
            f"fit=({event.lat_deg:.2f}, {event.lon_deg:.2f})  "
            f"RMS={event.rms_residual_ms:.2f} ms"
        )
        ax.grid(alpha=0.3)
        ax.legend(frameon=False, ncol=3, loc="best")

        txt = (
            f"tx={datetime.fromtimestamp(event.transmit_time_s, timezone.utc).strftime('%H:%M:%S.%f')[:-3]} UTC\n"
            f"rate={event.median_chirp_rate_hz_s/1e3:.0f} kHz/s\n"
            f"N={event.n_observations}\n"
            f"resid p5/p50/p95={np.percentile(residual_ms,5):.2f}/"
            f"{np.percentile(residual_ms,50):.2f}/"
            f"{np.percentile(residual_ms,95):.2f} ms"
        )
        ax.text(
            0.01,
            0.99,
            txt,
            transform=ax.transAxes,
            ha="left",
            va="top",
            fontsize=10,
            bbox={"facecolor": "white", "edgecolor": "0.8", "alpha": 0.85},
        )

        ofname = event_dir / (
            f"chirp_event_{event.chirp_second}_"
            f"{event.median_chirp_rate_hz_s/1e3:.0f}kHzs.png"
        )
        fig.savefig(ofname, dpi=180)
        plt.close(fig)

    print(f"wrote {len(events_to_plot)} individual event plots to {event_dir}")


def plot_multistation_statistics(
    stats: list[GroupStats],
    stations: list[str],
    output_dir: Path,
    date: str,
) -> None:
    if not stats:
        print("No multi-station statistics to plot")
        return

    combo_labels = []
    for stat in stats:
        label = "+".join(stat.stations)
        if label not in combo_labels:
            combo_labels.append(label)

    rates_khz = np.array([stat.median_chirp_rate_hz_s / 1e3 for stat in stats])
    rounded_rates = np.array([int(round(rate)) for rate in rates_khz])
    rate_values = sorted(set(rounded_rates))
    rate_labels = {100: "100 kHz/s", 125: "125 kHz/s (Australia/JORN)"}

    counts_by_combo_rate = {
        combo: {rate: 0 for rate in rate_values}
        for combo in combo_labels
    }
    three_station_stats = []
    all_stations = tuple(stations)
    for stat, rate in zip(stats, rounded_rates):
        combo = "+".join(stat.stations)
        counts_by_combo_rate[combo][rate] += 1
        if stat.stations == all_stations:
            three_station_stats.append(stat)

    fig, ax = plt.subplots(3, 1, figsize=(12, 12), constrained_layout=True)

    x = np.arange(len(combo_labels))
    width = min(0.8 / max(len(rate_values), 1), 0.35)
    colors = {100: "#4C78A8", 125: "#F58518"}
    for i, rate in enumerate(rate_values):
        vals = [counts_by_combo_rate[combo][rate] for combo in combo_labels]
        offset = (i - (len(rate_values) - 1) / 2.0) * width
        ax[0].bar(
            x + offset,
            vals,
            width=width,
            label=rate_labels.get(rate, f"{rate} kHz/s"),
            color=colors.get(rate),
        )
    ax[0].set_xticks(x)
    ax[0].set_xticklabels(combo_labels, rotation=30, ha="right")
    ax[0].set_ylabel("Same-second groups")
    ax[0].set_title("Multi-station chirp detections by station combination")
    ax[0].legend(frameon=False)
    ax[0].grid(axis="y", alpha=0.3)

    if three_station_stats:
        span_ms = np.array([stat.time_span_ms for stat in three_station_stats])
        nobs = np.array([stat.n_observations for stat in three_station_stats])
        rates3 = np.array([int(round(stat.median_chirp_rate_hz_s / 1e3)) for stat in three_station_stats])
        bins = np.linspace(0.0, min(250.0, max(1.0, np.percentile(span_ms, 99.0))), 45)
        for rate in sorted(set(rates3)):
            ax[1].hist(
                span_ms[rates3 == rate],
                bins=bins,
                histtype="step",
                linewidth=2,
                label=rate_labels.get(rate, f"{rate} kHz/s"),
                color=colors.get(rate),
            )
        ax[1].set_xlabel("Span of receive-time measurements in same group (ms)")
        ax[1].set_ylabel("Three-station groups")
        ax[1].set_title("Receive-time spread for groups seen at all three stations")
        ax[1].legend(frameon=False)
        ax[1].grid(alpha=0.3)

        ax[2].scatter(
            nobs,
            span_ms,
            c=[colors.get(rate, "0.4") for rate in rates3],
            s=26,
            alpha=0.75,
            edgecolor="none",
        )
        ax[2].set_xlabel("Number of detections in same-second group")
        ax[2].set_ylabel("Receive-time span (ms)")
        ax[2].set_title("Detection multiplicity vs. timing spread")
        ax[2].grid(alpha=0.3)
    else:
        ax[1].text(0.5, 0.5, "No groups detected at all required stations",
                   ha="center", va="center", transform=ax[1].transAxes)
        ax[2].axis("off")

    stats_file = output_dir / f"chirp_multistation_statistics_{date}.png"
    fig.savefig(stats_file, dpi=200)
    plt.close(fig)
    print(f"wrote {stats_file}")


def main() -> None:
    args = parse_args()
    stations = [s.strip() for s in args.stations.split(",") if s.strip()]
    if len(stations) < 3:
        raise ValueError("Transmitter localization requires detections from at least three stations")
    velocity_km_s = args.group_velocity_frac * C_KM_S
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    station_info = load_station_info(args.config)
    missing = [s for s in stations if s not in station_info]
    if missing:
        raise ValueError(f"Missing station coordinates in {args.config}: {missing}")

    groups = read_grouped_observations(args.data_dir, args.date, stations, args.min_snr)
    candidate_groups = sum(1 for v in groups.values() if set(v.keys()) == set(stations))
    print(f"same floor(chirp_time) three-station groups with all required stations: {candidate_groups}")
    print(f"effective group velocity: {velocity_km_s:.1f} km/s ({args.group_velocity_frac:.2f} c)")
    stats = group_statistics(groups, stations)
    plot_multistation_statistics(stats, stations, output_dir, args.date)

    events = locate_events(
        groups,
        stations,
        station_info,
        velocity_km_s,
        args.max_group_span_s,
        args.max_rms_residual_ms,
        args.max_source_distance_km,
    )
    print(f"retained located events: {len(events)}")

    csv_file = output_dir / f"chirp_source_locations_{args.date}.csv"
    write_csv(events, csv_file, stations)
    print(f"wrote {csv_file}")
    plot_events(events, station_info, stations, output_dir, args.date)
    plot_individual_events(events, stations, output_dir, args.date, args.event_plot_limit)


if __name__ == "__main__":
    main()
