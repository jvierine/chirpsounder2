#!/usr/bin/env python3
"""Interactive angle-of-arrival prototype for three-station chirp detections.

The input files are the consolidated ``cdetections-<station>-<block>.h5`` files
written by detections2metadata.py.  Each row is:

    chirp_time, receiver_time, detected_frequency, chirp_rate, snr

This first-pass tool finds detections where three or more receiver stations see
the same chirp rate during the same integer second.  For each such event it
plots the redundant frequency detections per station, estimates a plane-wave
arrival direction from inter-station timing differences, and shows that bearing
on a Cartopy map.
"""

from __future__ import annotations

import argparse
import configparser
import glob
import json
import math
import os
import re
import sys
import types
import warnings
from dataclasses import dataclass

try:
    import h5py
    import matplotlib.colors as mcolors
    import matplotlib.pyplot as plt
    import numpy as np
    from matplotlib.widgets import Button, Slider
except ImportError as exc:  # pragma: no cover - only used for friendly CLI error
    raise SystemExit(
        "Missing Python dependency: %s\n"
        "Install the chirpsounder2 plotting dependencies, e.g. h5py, numpy, "
        "matplotlib, and cartopy." % exc
    )


C = 299_792_458.0
EARTH_RADIUS_M = 6_371_000.0
DETECTION_RE = re.compile(r"cdetections-([^-]+)-([0-9]+)\.h5$")

warnings.filterwarnings(
    "ignore",
    message=r"invalid value encountered in create_collection",
    category=RuntimeWarning,
)


@dataclass
class DetectionTable:
    station: str
    data: np.ndarray


@dataclass
class Event:
    second: int
    chirp_rate: float
    stations: dict[str, np.ndarray]


@dataclass
class SegmentSolution:
    f0_hz: float
    f1_hz: float
    stations: dict[str, np.ndarray]
    solution: dict[str, float]


def import_cartopy():
    # Some non-interactive conda environments can crash in libreadline when
    # Cartopy imports pyshp, which imports doctest/pdb/rlcompleter.  Cartopy
    # does not need readline, so provide the tiny API rlcompleter expects.
    if "readline" not in sys.modules:
        readline_stub = types.ModuleType("readline")
        readline_stub.set_completer = lambda _completer: None
        sys.modules["readline"] = readline_stub

    try:
        import cartopy.crs as ccrs
        import cartopy.feature as cfeature
    except ImportError as exc:  # pragma: no cover
        raise SystemExit(
            "Missing Python dependency: %s\n"
            "This prototype uses Cartopy for the map panel." % exc
        )
    return ccrs, cfeature


def load_station_info(config_path: str) -> dict[str, dict[str, float]]:
    parser = configparser.ConfigParser()
    if not parser.read(config_path):
        raise FileNotFoundError(config_path)
    return json.loads(parser["stations"]["station_info"])


def station_from_filename(path: str) -> str:
    match = DETECTION_RE.search(os.path.basename(path))
    if not match:
        raise ValueError("cannot determine station from %s" % path)
    return match.group(1)


def detection_files(data_path: str) -> list[str]:
    if os.path.isdir(data_path):
        files = glob.glob(os.path.join(data_path, "cdetections-*.h5"))
        files += glob.glob(os.path.join(data_path, "2*", "cdetections-*.h5"))
    else:
        files = glob.glob(data_path)
    return sorted(set(files))


def read_detections(files: list[str]) -> list[DetectionTable]:
    tables = []
    for path in files:
        station = station_from_filename(path)
        with h5py.File(path, "r") as handle:
            data = np.asarray(handle["data"][:], dtype=np.float64)
        if data.size == 0:
            continue
        if data.ndim != 2 or data.shape[1] < 5:
            raise ValueError("%s has unexpected data shape %s" % (path, data.shape))
        tables.append(DetectionTable(station=station, data=data[:, :5]))
    return tables


def find_three_station_events(
    tables: list[DetectionTable],
    min_stations: int = 3,
    chirp_rate_round_hz: float = 1.0,
    max_events: int | None = None,
) -> list[Event]:
    grouped: dict[tuple[int, float], dict[str, list[np.ndarray]]] = {}
    for table in tables:
        seconds = np.floor(table.data[:, 0]).astype(np.int64)
        rates = np.round(table.data[:, 3] / chirp_rate_round_hz) * chirp_rate_round_hz
        for second, rate in sorted(set(zip(seconds.tolist(), rates.tolist()))):
            idx = np.where((seconds == second) & (rates == rate))[0]
            if len(idx) == 0:
                continue
            grouped.setdefault((int(second), float(rate)), {}).setdefault(
                table.station, []
            ).append(table.data[idx, :])

    events = []
    for (second, chirp_rate), by_station in sorted(grouped.items()):
        stations = {}
        for station, chunks in by_station.items():
            stations[station] = np.vstack(chunks)
        if len(stations) >= min_stations:
            events.append(Event(second=second, chirp_rate=chirp_rate, stations=stations))
            if max_events is not None and len(events) >= max_events:
                break
    return events


def latlon_to_enu_offsets(
    station_info: dict[str, dict[str, float]], stations: list[str]
) -> tuple[float, float, dict[str, tuple[float, float]]]:
    lats = np.array([station_info[s]["lat"] for s in stations], dtype=np.float64)
    lons = np.array([station_info[s]["lon"] for s in stations], dtype=np.float64)
    lat0 = float(np.mean(lats))
    lon0 = float(np.mean(lons))
    lat0_rad = math.radians(lat0)
    offsets = {}
    for station in stations:
        lat = station_info[station]["lat"]
        lon = station_info[station]["lon"]
        north = math.radians(lat - lat0) * EARTH_RADIUS_M
        east = math.radians(lon - lon0) * EARTH_RADIUS_M * math.cos(lat0_rad)
        offsets[station] = (east, north)
    return lat0, lon0, offsets


def representative_time(rows: np.ndarray) -> float:
    snr = rows[:, 4]
    times = rows[:, 0]
    good = np.isfinite(times) & np.isfinite(snr) & (snr > 0)
    if np.count_nonzero(good) >= 2:
        return float(np.average(times[good], weights=snr[good]))
    return float(np.median(times))


def estimate_aoa_from_station_rows(
    stations: dict[str, np.ndarray], station_info: dict[str, dict[str, float]], b: float
) -> dict[str, float]:
    station_names = sorted(stations)
    lat0, lon0, offsets = latlon_to_enu_offsets(station_info, station_names)
    times = {station: representative_time(stations[station]) for station in station_names}
    t_mean = float(np.mean(list(times.values())))

    # Fit t_i = t_ref + east_i*s_e + north_i*s_n.
    a = []
    y = []
    for station in station_names:
        east, north = offsets[station]
        a.append([1.0, east, north])
        y.append(times[station] - t_mean)
    a = np.asarray(a, dtype=np.float64)
    y = np.asarray(y, dtype=np.float64)
    model, *_ = np.linalg.lstsq(a, y, rcond=None)
    s_east = float(model[1])
    s_north = float(model[2])

    # The fitted time gradient points toward later arrivals.  The transmitter
    # direction is the opposite direction on the ground.
    bearing_to_source = (math.degrees(math.atan2(-s_east, -s_north)) + 360.0) % 360.0
    apparent_speed = math.inf
    slowness = math.hypot(s_east, s_north)
    if slowness > 0:
        apparent_speed = 1.0 / slowness

    tau = float(np.median([times[s] - math.floor(times[s]) for s in station_names]))
    distance_m = b * C * tau
    residual = y - a @ model
    return {
        "lat0": lat0,
        "lon0": lon0,
        "bearing_deg": bearing_to_source,
        "tau_s": tau,
        "distance_m": distance_m,
        "apparent_speed_mps": apparent_speed,
        "slowness_s_per_m": slowness,
        "rms_residual_s": float(np.sqrt(np.mean(residual**2))),
        "n_stations": len(station_names),
    }


def estimate_aoa(
    event: Event, station_info: dict[str, dict[str, float]], b: float
) -> dict[str, float]:
    return estimate_aoa_from_station_rows(event.stations, station_info, b)


def frequency_segment_solutions(
    event: Event,
    station_info: dict[str, dict[str, float]],
    b: float,
    bin_hz: float,
    min_stations: int,
) -> list[SegmentSolution]:
    all_freqs = np.concatenate([rows[:, 2] for rows in event.stations.values()])
    good_freqs = all_freqs[np.isfinite(all_freqs)]
    if good_freqs.size == 0:
        return []

    f_min = math.floor(float(np.min(good_freqs)) / bin_hz) * bin_hz
    f_max = math.ceil(float(np.max(good_freqs)) / bin_hz) * bin_hz
    segments = []
    for f0 in np.arange(f_min, f_max, bin_hz):
        f1 = float(f0 + bin_hz)
        segment_stations = {}
        for station, rows in event.stations.items():
            in_bin = (rows[:, 2] >= f0) & (rows[:, 2] < f1)
            if np.any(in_bin):
                segment_stations[station] = rows[in_bin, :]
        if len(segment_stations) < min_stations:
            continue
        solution = estimate_aoa_from_station_rows(segment_stations, station_info, b)
        segments.append(
            SegmentSolution(
                f0_hz=float(f0),
                f1_hz=f1,
                stations=segment_stations,
                solution=solution,
            )
        )
    return segments


def destination_point(lat_deg: float, lon_deg: float, bearing_deg: float, distance_m: float):
    lat1 = math.radians(lat_deg)
    lon1 = math.radians(lon_deg)
    brng = math.radians(bearing_deg)
    delta = distance_m / EARTH_RADIUS_M
    lat2 = math.asin(
        math.sin(lat1) * math.cos(delta)
        + math.cos(lat1) * math.sin(delta) * math.cos(brng)
    )
    lon2 = lon1 + math.atan2(
        math.sin(brng) * math.sin(delta) * math.cos(lat1),
        math.cos(delta) - math.sin(lat1) * math.sin(lat2),
    )
    lon2 = (math.degrees(lon2) + 540.0) % 360.0 - 180.0
    return math.degrees(lat2), lon2


def great_circle_points(lat: float, lon: float, bearing: float, npts: int = 721):
    distances = np.linspace(-math.pi * EARTH_RADIUS_M, math.pi * EARTH_RADIUS_M, npts)
    lats = []
    lons = []
    for distance in distances:
        use_bearing = bearing if distance >= 0 else (bearing + 180.0) % 360.0
        lat2, lon2 = destination_point(lat, lon, use_bearing, abs(float(distance)))
        lats.append(lat2)
        lons.append(lon2)
    return np.asarray(lats), np.asarray(lons)


class InteractiveAoA:
    def __init__(self, events, station_info, b, frequency_bin_hz, min_stations):
        self.ccrs, self.cfeature = import_cartopy()
        self.events = events
        self.station_info = station_info
        self.index = 0
        self.b = b
        self.frequency_bin_hz = frequency_bin_hz
        self.min_stations = min_stations
        self.fig = plt.figure(figsize=(14, 9))
        self.ax_time = self.fig.add_subplot(2, 1, 1)
        self.ax_map = self.fig.add_subplot(
            2, 1, 2, projection=self.ccrs.PlateCarree()
        )
        self.fig.subplots_adjust(bottom=0.16, hspace=0.28)
        self.prev_ax = self.fig.add_axes([0.35, 0.035, 0.08, 0.04])
        self.next_ax = self.fig.add_axes([0.45, 0.035, 0.08, 0.04])
        self.slider_ax = self.fig.add_axes([0.60, 0.045, 0.28, 0.025])
        self.prev_button = Button(self.prev_ax, "Prev")
        self.next_button = Button(self.next_ax, "Next")
        self.b_slider = Slider(self.slider_ax, "b", 0.70, 1.00, valinit=b, valstep=0.001)
        self.prev_button.on_clicked(self.prev)
        self.next_button.on_clicked(self.next)
        self.b_slider.on_changed(self.set_b)
        self.fig.canvas.mpl_connect("key_press_event", self.on_key)
        self.draw()

    def prev(self, _event=None):
        self.index = (self.index - 1) % len(self.events)
        self.draw()

    def next(self, _event=None):
        self.index = (self.index + 1) % len(self.events)
        self.draw()

    def set_b(self, value):
        self.b = float(value)
        self.draw()

    def on_key(self, event):
        if event.key in ("right", "n", " "):
            self.next()
        elif event.key in ("left", "p", "backspace"):
            self.prev()

    def draw(self):
        event = self.events[self.index]
        whole_event_solution = estimate_aoa(event, self.station_info, self.b)
        segments = frequency_segment_solutions(
            event,
            self.station_info,
            self.b,
            self.frequency_bin_hz,
            self.min_stations,
        )
        self.ax_time.clear()
        self.ax_map.clear()
        colors = plt.cm.viridis(np.linspace(0.05, 0.95, max(len(segments), 1)))

        for station, rows in sorted(event.stations.items()):
            dt_ms = (rows[:, 0] - event.second) * 1e3
            self.ax_time.scatter(
                rows[:, 2] / 1e6,
                dt_ms,
                s=np.clip(rows[:, 4], 8, 80),
                alpha=0.65,
                label=station,
            )
        for color, segment in zip(colors, segments):
            self.ax_time.axvspan(
                segment.f0_hz / 1e6,
                segment.f1_hz / 1e6,
                color=color,
                alpha=0.08,
                linewidth=0,
            )
        self.ax_time.set_xlabel("Detection frequency (MHz)")
        self.ax_time.set_ylabel("chirp_time - floor(chirp_time) (ms)")
        self.ax_time.grid(True, alpha=0.25)
        self.ax_time.legend(loc="best")
        self.ax_time.set_title(
            "Event %d/%d  second=%d  chirp_rate=%.3f kHz/s  b=%.3f  "
            "%d x %.0f kHz segments; whole-event bearing %.1f deg, R %.0f km"
            % (
                self.index + 1,
                len(self.events),
                event.second,
                event.chirp_rate / 1e3,
                self.b,
                len(segments),
                self.frequency_bin_hz / 1e3,
                whole_event_solution["bearing_deg"],
                whole_event_solution["distance_m"] / 1e3,
            )
        )

        self.ax_map.set_global()
        self.ax_map.add_feature(self.cfeature.LAND, facecolor="0.92")
        self.ax_map.add_feature(self.cfeature.OCEAN, facecolor="0.86")
        self.ax_map.add_feature(self.cfeature.COASTLINE, linewidth=0.5)
        self.ax_map.add_feature(self.cfeature.BORDERS, linewidth=0.3)
        self.ax_map.gridlines(draw_labels=True, linewidth=0.3, color="0.4", alpha=0.6)

        used_stations = sorted(event.stations)
        for station in used_stations:
            info = self.station_info[station]
            self.ax_map.scatter(info["lon"], info["lat"], marker="^", s=70, color="k")
            self.ax_map.text(info["lon"] + 1.0, info["lat"] + 0.7, station, fontsize=9)

        src_lats = []
        src_lons = []
        labels = []
        for color, segment in zip(colors, segments):
            solution = segment.solution
            lats, lons = great_circle_points(
                solution["lat0"], solution["lon0"], solution["bearing_deg"]
            )
            label = "%.1f-%.1f MHz" % (segment.f0_hz / 1e6, segment.f1_hz / 1e6)
            self.ax_map.plot(
                lons,
                lats,
                color=color,
                linewidth=1.1,
                alpha=0.70,
                transform=self.ccrs.Geodetic(),
            )
            src_lat, src_lon = destination_point(
                solution["lat0"],
                solution["lon0"],
                solution["bearing_deg"],
                solution["distance_m"],
            )
            src_lats.append(src_lat)
            src_lons.append(src_lon)
            labels.append(label)
        if src_lats:
            scatter = self.ax_map.scatter(
                src_lons,
                src_lats,
                c=np.arange(len(src_lats)),
                cmap="viridis",
                marker="o",
                s=65,
                edgecolor="k",
                linewidth=0.4,
                zorder=5,
                transform=self.ccrs.PlateCarree(),
            )
            for src_lat, src_lon, label in zip(src_lats, src_lons, labels):
                self.ax_map.text(
                    src_lon + 1.2,
                    src_lat + 0.7,
                    label,
                    fontsize=7,
                    transform=self.ccrs.PlateCarree(),
                )
        self.ax_map.set_title(
            "Plane-wave AoA by frequency segment from %s"
            % (
                ", ".join(used_stations),
            )
        )
        self.fig.canvas.draw_idle()


def collect_segment_positions(
    events: list[Event],
    station_info: dict[str, dict[str, float]],
    b: float,
    frequency_bin_hz: float,
    min_stations: int,
) -> tuple[np.ndarray, np.ndarray]:
    lats = []
    lons = []
    for event in events:
        segments = frequency_segment_solutions(
            event, station_info, b, frequency_bin_hz, min_stations
        )
        for segment in segments:
            solution = segment.solution
            lat, lon = destination_point(
                solution["lat0"],
                solution["lon0"],
                solution["bearing_deg"],
                solution["distance_m"],
            )
            if np.isfinite(lat) and np.isfinite(lon):
                lats.append(lat)
                lons.append(lon)
    return np.asarray(lats, dtype=np.float64), np.asarray(lons, dtype=np.float64)


def collect_segment_paths(
    events: list[Event],
    station_info: dict[str, dict[str, float]],
    b: float,
    frequency_bin_hz: float,
    min_stations: int,
    path_points: int,
) -> tuple[np.ndarray, np.ndarray, int]:
    lats = []
    lons = []
    n_segments = 0
    for event in events:
        segments = frequency_segment_solutions(
            event, station_info, b, frequency_bin_hz, min_stations
        )
        for segment in segments:
            solution = segment.solution
            path_lats, path_lons = great_circle_points(
                solution["lat0"],
                solution["lon0"],
                solution["bearing_deg"],
                npts=path_points,
            )
            good = np.isfinite(path_lats) & np.isfinite(path_lons)
            lats.append(path_lats[good])
            lons.append(path_lons[good])
            n_segments += 1
    if not lats:
        return np.array([], dtype=np.float64), np.array([], dtype=np.float64), 0
    return np.concatenate(lats), np.concatenate(lons), n_segments


def plot_position_heatmap(
    lats: np.ndarray,
    lons: np.ndarray,
    station_info: dict[str, dict[str, float]],
    b: float,
    frequency_bin_hz: float,
    chirp_rate_hz: float | None,
    resolution_deg: float,
    output: str | None,
    title_prefix: str,
    count_label: str,
):
    ccrs, cfeature = import_cartopy()
    lon_edges = np.arange(-180.0, 180.0 + resolution_deg, resolution_deg)
    lat_edges = np.arange(-90.0, 90.0 + resolution_deg, resolution_deg)
    counts, _, _ = np.histogram2d(lats, lons, bins=(lat_edges, lon_edges))
    counts = np.ma.masked_where(counts <= 0, counts)

    fig = plt.figure(figsize=(15, 8))
    ax = fig.add_subplot(1, 1, 1, projection=ccrs.PlateCarree())
    ax.set_global()
    ax.add_feature(cfeature.LAND, facecolor="0.93")
    ax.add_feature(cfeature.OCEAN, facecolor="0.86")
    ax.add_feature(cfeature.COASTLINE, linewidth=0.5)
    ax.add_feature(cfeature.BORDERS, linewidth=0.3)
    ax.gridlines(draw_labels=True, linewidth=0.3, color="0.4", alpha=0.6)
    mesh = ax.pcolormesh(
        lon_edges,
        lat_edges,
        counts,
        cmap="inferno",
        norm=mcolors.LogNorm(vmin=1, vmax=max(1, int(np.max(counts)))),
        transform=ccrs.PlateCarree(),
    )
    cbar = fig.colorbar(mesh, ax=ax, orientation="horizontal", pad=0.06, shrink=0.82)
    cbar.set_label("%s per %.2f deg bin" % (count_label, resolution_deg))

    for station in ("DOB", "KHO", "TGO"):
        if station not in station_info:
            continue
        info = station_info[station]
        ax.scatter(
            info["lon"],
            info["lat"],
            marker="^",
            s=80,
            color="cyan",
            edgecolor="k",
            linewidth=0.6,
            zorder=5,
            transform=ccrs.PlateCarree(),
        )
        ax.text(
            info["lon"] + 1.0,
            info["lat"] + 0.7,
            station,
            fontsize=9,
            color="k",
            transform=ccrs.PlateCarree(),
        )
    ax.set_title(
        "%s: %d samples, b=%.2f, "
        "%.0f kHz bins%s"
        % (
            title_prefix,
            len(lats),
            b,
            frequency_bin_hz / 1e3,
            ""
            if chirp_rate_hz is None
            else ", chirp rate %.0f kHz/s" % (chirp_rate_hz / 1e3),
        )
    )
    fig.tight_layout()
    if output:
        fig.savefig(output, dpi=180)
        print("wrote %s" % output)
    else:
        plt.show()


def main():
    parser = argparse.ArgumentParser(
        description="Interactive AoA/range prototype for three-station chirp detections."
    )
    parser.add_argument(
        "data",
        nargs="?",
        default="/Users/j/data/2026-05-20",
        help="Directory or glob containing cdetections-*.h5 files.",
    )
    parser.add_argument(
        "--station-config",
        default="examples/marieluise/server.ini",
        help="Config file containing [stations] station_info.",
    )
    parser.add_argument("--b", type=float, default=0.85, help="Group velocity factor.")
    parser.add_argument(
        "--frequency-bin-hz",
        type=float,
        default=500e3,
        help="Frequency segment width for independent AoA estimates.",
    )
    parser.add_argument("--max-events", type=int, default=None)
    parser.add_argument("--min-stations", type=int, default=3)
    parser.add_argument(
        "--heatmap",
        action="store_true",
        help="Plot a heat map from all per-frequency-segment AoA estimates.",
    )
    parser.add_argument(
        "--heatmap-kind",
        choices=("positions", "paths"),
        default="positions",
        help="Use range position points or sampled great-circle paths for --heatmap.",
    )
    parser.add_argument(
        "--heatmap-output",
        default=None,
        help="Output PNG/PDF path for --heatmap. If omitted, show interactively.",
    )
    parser.add_argument(
        "--heatmap-resolution-deg",
        type=float,
        default=2.0,
        help="Lat/lon bin size for --heatmap.",
    )
    parser.add_argument(
        "--chirp-rate-hz",
        type=float,
        default=None,
        help="Only use events with this chirp rate in Hz/s, e.g. 100000 or 125000.",
    )
    parser.add_argument(
        "--path-points",
        type=int,
        default=721,
        help="Number of sampled points per great circle for --heatmap-kind paths.",
    )
    args = parser.parse_args()

    station_info = load_station_info(args.station_config)
    files = detection_files(args.data)
    if not files:
        raise SystemExit("No cdetections files found under %s" % args.data)
    tables = read_detections(files)
    tables = [t for t in tables if t.station in station_info]
    events = find_three_station_events(tables, min_stations=args.min_stations)
    events = [
        event
        for event in events
        if frequency_segment_solutions(
            event,
            station_info,
            args.b,
            args.frequency_bin_hz,
            args.min_stations,
        )
    ]
    if args.max_events is not None:
        events = events[: args.max_events]
    if args.chirp_rate_hz is not None:
        events = [
            event
            for event in events
            if np.isclose(event.chirp_rate, args.chirp_rate_hz, rtol=0.0, atol=1.0)
        ]
    if not events:
        raise SystemExit(
            "No events with at least %d stations and one %.0f kHz overlapping "
            "frequency segment found."
            % (args.min_stations, args.frequency_bin_hz / 1e3)
        )
    print(
        "read %d files, found %d events with at least one %.0f kHz "
        "three-station frequency segment"
        % (len(files), len(events), args.frequency_bin_hz / 1e3)
    )
    if args.heatmap:
        if args.heatmap_kind == "paths":
            lats, lons, n_segments = collect_segment_paths(
                events,
                station_info,
                args.b,
                args.frequency_bin_hz,
                args.min_stations,
                args.path_points,
            )
            title_prefix = "Chirp AoA great-circle path heat map"
            count_label = "Sampled path points"
            print(
                "heat map uses %d sampled path points from %d segment great circles"
                % (len(lats), n_segments)
            )
        else:
            lats, lons = collect_segment_positions(
                events,
                station_info,
                args.b,
                args.frequency_bin_hz,
                args.min_stations,
            )
            title_prefix = "Chirp AoA/range position heat map"
            count_label = "Position determinations"
            print("heat map uses %d segment position estimates" % len(lats))
        if len(lats) == 0:
            raise SystemExit("No segment samples available for heat map.")
        plot_position_heatmap(
            lats,
            lons,
            station_info,
            args.b,
            args.frequency_bin_hz,
            args.chirp_rate_hz,
            args.heatmap_resolution_deg,
            args.heatmap_output,
            title_prefix,
            count_label,
        )
        return
    InteractiveAoA(
        events,
        station_info,
        args.b,
        args.frequency_bin_hz,
        args.min_stations,
    )
    plt.show()


if __name__ == "__main__":
    main()
