#!/usr/bin/env python3
"""Make chirp-time-band AoA dashboard plots.

For each 20 ms band of fractional chirp time, this script:

* filters detections using the plot_detectionfiles.py logic:
  floor(chirp_time), chirp_rate, and +/-33 ms about the group median;
* keeps only bands with more than a configurable number of detections per day;
* estimates AoA/range for the detections in that band using the three-station
  timing code from chirp_aoa_interactive.py;
* renders a 2x2 figure with the detection band, position scatter, position heat
  map, and great-circle path heat map.
"""

from __future__ import annotations

import argparse
import datetime as dt
import os

import matplotlib.colors as mcolors
import matplotlib.dates as mdates
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

import chirp_aoa_interactive as aoa
from plot_detectionfiles import (
    detection_filter_indices_by_floor_time_and_rate,
    parse_utc_datetime,
)

PLOT_RC = {
    "font.size": 14,
    "axes.labelsize": 15,
    "axes.titlesize": 16,
    "xtick.labelsize": 12,
    "ytick.labelsize": 12,
    "legend.fontsize": 12,
    "figure.titlesize": 18,
}


def read_all_tables(data_path: str, station_info: dict[str, dict[str, float]]):
    files = aoa.detection_files(data_path)
    tables = aoa.read_detections(files)
    return [table for table in tables if table.station in station_info]


def time_filter_table(table: aoa.DetectionTable, start_s: float, end_s: float):
    idx = (table.data[:, 0] >= start_s) & (table.data[:, 0] < end_s)
    return aoa.DetectionTable(table.station, table.data[idx, :])


def quality_filter_table(table: aoa.DetectionTable, min_detections: int, max_dt: float):
    if table.data.size == 0:
        return table
    idx = detection_filter_indices_by_floor_time_and_rate(
        table.data, min_detections=min_detections, max_dt=max_dt
    )
    return aoa.DetectionTable(table.station, table.data[idx, :])


def band_filter_table(table: aoa.DetectionTable, band0_ms: float, band1_ms: float):
    if table.data.size == 0:
        return table
    frac_ms = (table.data[:, 0] - np.floor(table.data[:, 0])) * 1e3
    idx = (frac_ms >= band0_ms) & (frac_ms < band1_ms)
    return aoa.DetectionTable(table.station, table.data[idx, :])


def filtered_day_tables(
    tables: list[aoa.DetectionTable],
    start_s: float,
    end_s: float,
    min_detections: int,
    max_dt: float,
):
    filtered = []
    for table in tables:
        table = time_filter_table(table, start_s, end_s)
        table = quality_filter_table(table, min_detections, max_dt)
        if table.data.size:
            filtered.append(table)
    return filtered


def concatenate_tables(tables: list[aoa.DetectionTable]) -> np.ndarray:
    chunks = [table.data for table in tables if table.data.size]
    if not chunks:
        return np.empty((0, 5), dtype=np.float64)
    return np.vstack(chunks)


def concatenate_station_tables(
    tables: list[aoa.DetectionTable], station: str
) -> np.ndarray:
    chunks = [table.data for table in tables if table.station == station and table.data.size]
    if not chunks:
        return np.empty((0, 5), dtype=np.float64)
    return np.vstack(chunks)


def eligible_bands(
    tables: list[aoa.DetectionTable],
    band_ms: float,
    min_band_detections: int,
    band_station: str | None = None,
):
    data = (
        concatenate_station_tables(tables, band_station)
        if band_station is not None
        else concatenate_tables(tables)
    )
    if data.size == 0:
        return []
    frac_ms = (data[:, 0] - np.floor(data[:, 0])) * 1e3
    bands = []
    for band0 in np.arange(0.0, 1000.0, band_ms):
        band1 = min(1000.0, band0 + band_ms)
        count = int(np.count_nonzero((frac_ms >= band0) & (frac_ms < band1)))
        if count > min_band_detections:
            bands.append((float(band0), float(band1), count))
    return bands


def band_segment_solutions(
    tables: list[aoa.DetectionTable],
    station_info: dict[str, dict[str, float]],
    band0_ms: float,
    band1_ms: float,
    b: float,
    frequency_bin_hz: float,
    min_stations: int,
):
    band_tables = [
        band_filter_table(table, band0_ms, band1_ms)
        for table in tables
    ]
    band_tables = [table for table in band_tables if table.data.size]
    events = aoa.find_three_station_events(band_tables, min_stations=min_stations)
    segments = []
    for event in events:
        segments.extend(
            aoa.frequency_segment_solutions(
                event, station_info, b, frequency_bin_hz, min_stations
            )
        )
    return segments


def segment_positions(segments: list[aoa.SegmentSolution]):
    lats = []
    lons = []
    freqs_mhz = []
    for segment in segments:
        solution = segment.solution
        lat, lon = aoa.destination_point(
            solution["lat0"],
            solution["lon0"],
            solution["bearing_deg"],
            solution["distance_m"],
        )
        if np.isfinite(lat) and np.isfinite(lon):
            lats.append(lat)
            lons.append(lon)
            freqs_mhz.append(0.5 * (segment.f0_hz + segment.f1_hz) / 1e6)
    return (
        np.asarray(lats, dtype=np.float64),
        np.asarray(lons, dtype=np.float64),
        np.asarray(freqs_mhz, dtype=np.float64),
    )


def split_segments_by_range(
    segments: list[aoa.SegmentSolution], max_range_m: float
) -> tuple[list[aoa.SegmentSolution], list[aoa.SegmentSolution]]:
    valid = []
    over_range = []
    for segment in segments:
        if segment.solution["distance_m"] <= max_range_m:
            valid.append(segment)
        else:
            over_range.append(segment)
    return valid, over_range


def segment_path_samples(
    segments: list[aoa.SegmentSolution], path_points: int
):
    lats = []
    lons = []
    for segment in segments:
        solution = segment.solution
        path_lats, path_lons = aoa.great_circle_points(
            solution["lat0"],
            solution["lon0"],
            solution["bearing_deg"],
            npts=path_points,
        )
        good = np.isfinite(path_lats) & np.isfinite(path_lons)
        lats.append(path_lats[good])
        lons.append(path_lons[good])
    if not lats:
        return np.array([], dtype=np.float64), np.array([], dtype=np.float64)
    return np.concatenate(lats), np.concatenate(lons)


def draw_station_markers(ax, station_info, ccrs):
    for station in ("DOB", "KHO", "TGO"):
        if station not in station_info:
            continue
        info = station_info[station]
        ax.scatter(
            info["lon"],
            info["lat"],
            marker="^",
            s=45,
            color="cyan",
            edgecolor="k",
            linewidth=0.5,
            transform=ccrs.PlateCarree(),
            zorder=6,
        )


def draw_map_background(ax, ccrs, cfeature):
    ax.set_global()
    ax.add_feature(cfeature.LAND, facecolor="0.93")
    ax.add_feature(cfeature.OCEAN, facecolor="0.86")
    ax.add_feature(cfeature.COASTLINE, linewidth=0.35)
    ax.add_feature(cfeature.BORDERS, linewidth=0.2)
    ax.gridlines(draw_labels=False, linewidth=0.25, color="0.4", alpha=0.45)


def draw_frequency_colored_paths(ax, segments, ccrs, cmap="rainbow", path_points=361):
    if not segments:
        return None
    freqs_mhz = np.asarray(
        [0.5 * (segment.f0_hz + segment.f1_hz) / 1e6 for segment in segments],
        dtype=np.float64,
    )
    norm = mcolors.Normalize(
        vmin=float(np.nanmin(freqs_mhz)),
        vmax=float(np.nanmax(freqs_mhz)) if np.nanmax(freqs_mhz) > np.nanmin(freqs_mhz)
        else float(np.nanmin(freqs_mhz) + 1.0),
    )
    colormap = plt.get_cmap(cmap)
    for segment, freq_mhz in zip(segments, freqs_mhz):
        solution = segment.solution
        lats, lons = aoa.great_circle_points(
            solution["lat0"],
            solution["lon0"],
            solution["bearing_deg"],
            npts=path_points,
        )
        ax.plot(
            lons,
            lats,
            color=colormap(norm(freq_mhz)),
            linewidth=0.65,
            alpha=0.65,
            transform=ccrs.Geodetic(),
        )
    return plt.cm.ScalarMappable(norm=norm, cmap=colormap)


def draw_heatmap(ax, lats, lons, ccrs, resolution_deg=2.0):
    if len(lats) == 0:
        ax.text(0.5, 0.5, "No samples", ha="center", va="center", transform=ax.transAxes)
        return None
    lon_edges = np.arange(-180.0, 180.0 + resolution_deg, resolution_deg)
    lat_edges = np.arange(-90.0, 90.0 + resolution_deg, resolution_deg)
    counts, _, _ = np.histogram2d(lats, lons, bins=(lat_edges, lon_edges))
    counts = np.ma.masked_where(counts <= 0, counts)
    return ax.pcolormesh(
        lon_edges,
        lat_edges,
        counts,
        cmap="inferno",
        norm=mcolors.LogNorm(vmin=1, vmax=max(1, int(np.max(counts)))),
        transform=ccrs.PlateCarree(),
    )


def plot_band_dashboard(
    tables: list[aoa.DetectionTable],
    station_info: dict[str, dict[str, float]],
    start_time: dt.datetime,
    end_time: dt.datetime,
    band0_ms: float,
    band1_ms: float,
    band_count: int,
    segments: list[aoa.SegmentSolution],
    b: float,
    output: str,
    plot_station: str,
    heatmap_resolution_deg: float,
    path_points: int,
):
    ccrs, cfeature = aoa.import_cartopy()
    data = concatenate_station_tables(tables, plot_station)
    if data.size == 0:
        data = concatenate_tables(tables)
    frac_ms = (data[:, 0] - np.floor(data[:, 0])) * 1e3
    top_data = data
    top_frac_ms = frac_ms

    # A unique surface great-circle transmitter position only exists out to
    # the antipode.  Larger group-delay ranges wrap around the Earth and are
    # better represented as AoA great-circle constraints.
    max_position_range_m = np.pi * aoa.EARTH_RADIUS_M
    position_segments, over_range_segments = split_segments_by_range(
        segments, max_position_range_m
    )
    pos_lats, pos_lons, freqs_mhz = segment_positions(position_segments)
    path_lats, path_lons = segment_path_samples(segments, path_points)

    plt.rcParams.update(PLOT_RC)
    fig = plt.figure(figsize=(16, 10), constrained_layout=True)
    gs = fig.add_gridspec(2, 2)
    ax_det = fig.add_subplot(gs[0, 0])
    ax_scatter = fig.add_subplot(gs[0, 1], projection=ccrs.PlateCarree())
    ax_pos_heat = fig.add_subplot(gs[1, 0], projection=ccrs.PlateCarree())
    ax_path_heat = fig.add_subplot(gs[1, 1], projection=ccrs.PlateCarree())

    if top_data.size:
        times = pd.to_datetime(top_data[:, 0], unit="s", utc=True)
        sc = ax_det.scatter(
            times,
            top_frac_ms,
            c=top_data[:, 2] / 1e6,
            s=1.0,
            alpha=0.65,
            cmap="rainbow",
            vmin=5,
            vmax=25,
            linewidths=0,
        )
        cbar = fig.colorbar(sc, ax=ax_det, pad=0.01)
        cbar.set_label("Frequency (MHz)")
    ax_det.axhspan(band0_ms, band1_ms, color="0.70", alpha=0.8, zorder=0)
    ax_det.set_xlim(start_time, end_time)
    ax_det.set_ylim(0, 200)
    ax_det.set_xlabel("UTC time")
    ax_det.set_ylabel(r"$t_0-\lfloor t_0\rfloor$ (ms)")
    ax_det.set_title("Detections")
    ax_det.grid(True, alpha=0.25)
    ax_det.xaxis.set_major_formatter(mdates.DateFormatter("%H:%M"))
    plt.setp(ax_det.get_xticklabels(), rotation=45, ha="right")

    draw_map_background(ax_scatter, ccrs, cfeature)
    if len(pos_lats):
        sc2 = ax_scatter.scatter(
            pos_lons,
            pos_lats,
            c=freqs_mhz,
            cmap="rainbow",
            vmin=5,
            vmax=25,
            s=14,
            alpha=0.75,
            transform=ccrs.PlateCarree(),
        )
        cbar = fig.colorbar(sc2, ax=ax_scatter, orientation="horizontal", pad=0.04)
        cbar.set_label("Segment center frequency (MHz)")
    if over_range_segments:
        mappable = draw_frequency_colored_paths(
            ax_scatter, over_range_segments, ccrs, path_points=path_points
        )
        if mappable is not None and not len(pos_lats):
            cbar = fig.colorbar(mappable, ax=ax_scatter, orientation="horizontal", pad=0.04)
            cbar.set_label("Segment center frequency (MHz)")
    draw_station_markers(ax_scatter, station_info, ccrs)
    if over_range_segments and not len(pos_lats):
        ax_scatter.set_title("Great-circle paths")
    else:
        ax_scatter.set_title("Position estimates")

    draw_map_background(ax_pos_heat, ccrs, cfeature)
    mesh_pos = draw_heatmap(
        ax_pos_heat, pos_lats, pos_lons, ccrs, resolution_deg=heatmap_resolution_deg
    )
    if mesh_pos is not None:
        cbar = fig.colorbar(mesh_pos, ax=ax_pos_heat, orientation="horizontal", pad=0.04)
        cbar.set_label("Position estimates per bin")
    draw_station_markers(ax_pos_heat, station_info, ccrs)
    if len(pos_lats):
        ax_pos_heat.set_title("Position density")
    else:
        ax_pos_heat.set_title("Position density")

    draw_map_background(ax_path_heat, ccrs, cfeature)
    mesh_path = draw_heatmap(
        ax_path_heat, path_lats, path_lons, ccrs, resolution_deg=heatmap_resolution_deg
    )
    if mesh_path is not None:
        cbar = fig.colorbar(mesh_path, ax=ax_path_heat, orientation="horizontal", pad=0.04)
        cbar.set_label("Sampled great-circle path points per bin")
    draw_station_markers(ax_path_heat, station_info, ccrs)
    ax_path_heat.set_title("Path density")
    fig.savefig(output, dpi=170)
    plt.close(fig)
    print("wrote %s" % output)


def main():
    parser = argparse.ArgumentParser(description="Plot chirp-time-band AoA dashboards.")
    parser.add_argument("data", nargs="?", default="/Users/j/data/2026-05-20")
    parser.add_argument("--station-config", default="examples/marieluise/server.ini")
    parser.add_argument("--start", default="2026-05-20")
    parser.add_argument("--end", default=None)
    parser.add_argument("--b", type=float, default=0.85)
    parser.add_argument("--band-ms", type=float, default=20.0)
    parser.add_argument("--band", type=float, default=None, help="Band start in ms.")
    parser.add_argument("--min-band-detections", type=int, default=100)
    parser.add_argument("--min-detections", type=int, default=10)
    parser.add_argument("--max-dt", type=float, default=0.033)
    parser.add_argument("--frequency-bin-hz", type=float, default=500e3)
    parser.add_argument("--min-stations", type=int, default=3)
    parser.add_argument("--plot-station", default="DOB")
    parser.add_argument(
        "--band-station",
        default=None,
        help="Station used for selecting populated chirp-time bands. Defaults to --plot-station.",
    )
    parser.add_argument("--heatmap-resolution-deg", type=float, default=2.0)
    parser.add_argument("--path-points", type=int, default=361)
    parser.add_argument("--output-dir", default="/tmp/chirp_band_aoa")
    parser.add_argument("--max-bands", type=int, default=None)
    args = parser.parse_args()

    start_time = parse_utc_datetime(args.start)
    end_time = parse_utc_datetime(args.end) if args.end else start_time + dt.timedelta(days=1)
    if args.end and len(args.end) == 10:
        end_time += dt.timedelta(days=1)
    if end_time <= start_time:
        raise SystemExit("--end must be after --start")

    station_info = aoa.load_station_info(args.station_config)
    tables = read_all_tables(args.data, station_info)
    day_tables = filtered_day_tables(
        tables,
        start_time.timestamp(),
        end_time.timestamp(),
        args.min_detections,
        args.max_dt,
    )
    band_station = args.band_station or args.plot_station
    bands = eligible_bands(
        day_tables, args.band_ms, args.min_band_detections, band_station=band_station
    )
    if args.band is not None:
        band0 = float(args.band)
        band1 = min(1000.0, band0 + args.band_ms)
        data = concatenate_station_tables(day_tables, band_station)
        frac_ms = (data[:, 0] - np.floor(data[:, 0])) * 1e3
        count = int(np.count_nonzero((frac_ms >= band0) & (frac_ms < band1)))
        bands = [(band0, band1, count)]
    if args.max_bands is not None:
        bands = bands[: args.max_bands]
    if not bands:
        raise SystemExit(
            "No chirp-time bands passed the detection threshold for %s." % band_station
        )

    os.makedirs(args.output_dir, exist_ok=True)
    print("plotting %d chirp-time bands selected from %s" % (len(bands), band_station))
    for band0_ms, band1_ms, band_count in bands:
        segments = band_segment_solutions(
            day_tables,
            station_info,
            band0_ms,
            band1_ms,
            args.b,
            args.frequency_bin_hz,
            args.min_stations,
        )
        if not segments:
            print("skipping %.0f-%.0f ms: no three-station AoA estimates" % (
                band0_ms, band1_ms))
            continue
        output = os.path.join(
            args.output_dir,
            "chirp_band_aoa_%s_%03d_%03dms.png"
            % (start_time.strftime("%Y-%m-%d"), int(band0_ms), int(band1_ms)),
        )
        plot_band_dashboard(
            day_tables,
            station_info,
            start_time,
            end_time,
            band0_ms,
            band1_ms,
            band_count,
            segments,
            args.b,
            output,
            args.plot_station,
            args.heatmap_resolution_deg,
            args.path_points,
        )


if __name__ == "__main__":
    main()
