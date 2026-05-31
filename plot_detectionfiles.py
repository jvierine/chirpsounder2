import h5py
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
import glob
import os
import numpy as n
import scipy.constants as sc
#from datetime import datetime, timedelta
from datetime import datetime, timedelta, timezone
import chirp_config as cc
import propagation
import sys
import traceback
#plt.style.use('dark_background')
try:
    import psutil
except ImportError:
    psutil = None


def set_low_priority():
    if psutil is None:
        return
    try:
        p = psutil.Process()
        # Set I/O priority to idle (lowest) to avoid interrupting realtime processes.
        p.ionice(psutil.IOPRIO_CLASS_IDLE)
        p.nice(19)
    except Exception as exc:
        print("could not lower plot_detectionfiles.py priority: %s" % exc)

labels={100:"US (ROTHR)",125:"Australia (JORN)"}
UNIX_EPOCH_MPL = mdates.date2num(datetime(1970, 1, 1, tzinfo=timezone.utc))


def unix_to_mpl_dates(times):
    return UNIX_EPOCH_MPL + n.asarray(times, dtype=float) / 86400.0


def parse_utc_day(day):
    return datetime.strptime(day, "%Y-%m-%d").replace(tzinfo=timezone.utc)


def parse_utc_datetime(value):
    if len(value) == 10:
        return parse_utc_day(value)
    if value.endswith("Z"):
        value = value[:-1] + "+00:00"
    parsed = datetime.fromisoformat(value)
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def utc_date_dirs(start_t, end_t):
    day_start = n.floor(start_t/24/3600)*24*3600
    day_end = n.floor(end_t/24/3600)*24*3600
    dirs = []
    day_t = day_start
    while day_t <= day_end:
        dirs.append(datetime.fromtimestamp(day_t, timezone.utc).strftime("%Y-%m-%d"))
        day_t += 24*3600
    return dirs


def detection_files(data_dir, max_files=None, station_name=None, date_dirs=None):
    search_dirs = [data_dir]
    if date_dirs is None:
        now = datetime.now(timezone.utc)
        date_dirs = [
            (now - timedelta(days=day_offset)).strftime("%Y-%m-%d")
            for day_offset in range(3)
        ]
    search_dirs.extend(os.path.join(data_dir, dirname) for dirname in date_dirs)

    if station_name:
        filename_pattern = "cdetections-%s-*.h5" % (station_name)
    else:
        filename_pattern = "cdetections*.h5"
    files = []
    for search_dir in search_dirs:
        files += glob.glob(os.path.join(search_dir, filename_pattern))
    files = sorted(set(files))
    if max_files is not None:
        files = files[-max_files:]
    return files


def read_detection_files(files):
    dfs = []
    for f in files:
        print(f)
        try:
            with h5py.File(f, "r") as h:
                data = h["data"][()]
        except Exception as exc:
            print("skipping unreadable detection file %s: %s" % (f, exc))
            continue
        data = n.asarray(data)
        if data.ndim != 2 or data.shape[1] < 4:
            print("skipping malformed detection file %s shape=%s" % (f, data.shape))
            continue
        dfs.append(data)
    if len(dfs) == 0:
        return n.empty((0, 5))
    return n.concatenate(dfs, axis=0)


def detection_filter_indices(times, min_detections=5, max_dt=0.033):
    rtimes = n.round(times)
    order = n.argsort(rtimes, kind="mergesort")
    sorted_rtimes = rtimes[order]
    group_start = n.r_[0, n.flatnonzero(n.diff(sorted_rtimes)) + 1]
    group_end = n.r_[group_start[1:], len(order)]
    keep = []
    for start, end in zip(group_start, group_end):
        idx0 = order[start:end]
        if len(idx0) <= min_detections:
            continue
        median_time = n.median(times[idx0])
        idx = idx0[n.abs(times[idx0] - median_time) < max_dt]
        if len(idx) > min_detections:
            keep.append(idx)
    if len(keep) == 0:
        return n.array([], dtype=int)
    return n.concatenate(keep)


def detection_filter_indices_by_floor_time_and_rate(
        dfs, min_detections=5, max_dt=0.033):
    seconds = n.floor(dfs[:, 0]).astype(n.int64)
    rates = n.round(dfs[:, 3]).astype(n.int64)
    keys = n.column_stack((seconds, rates))
    _, inverse = n.unique(keys, axis=0, return_inverse=True)
    keep = []
    for group in n.unique(inverse):
        idx0 = n.where(inverse == group)[0]
        if len(idx0) < min_detections:
            continue
        median_time = n.median(dfs[idx0, 0])
        idx = idx0[n.abs(dfs[idx0, 0] - median_time) <= max_dt]
        if len(idx) >= min_detections:
            keep.append(idx)
    if len(keep) == 0:
        return n.array([], dtype=int)
    return n.concatenate(keep)


def needs_daily_plot(pfname, now=None):
    if now is None:
        import time
        now = time.time()
    if not os.path.exists(pfname):
        return True
    day_start = n.floor(now/24/3600)*24*3600
    return os.path.getmtime(pfname) < day_start


def format_time_span(start_t, n_hours, title_span=None):
    day_start = datetime.fromtimestamp(start_t, timezone.utc)
    day_end = datetime.fromtimestamp(start_t+n_hours*3600, timezone.utc)
    if title_span is not None:
        time_span_str = title_span
    elif n_hours <= 24:
        time_span_str = day_start.strftime("%Y-%m-%d UTC")
    else:
        time_span_str = "%s to %s UTC" % (
            day_start.strftime("%Y-%m-%d %H:%M"),
            day_end.strftime("%Y-%m-%d %H:%M"),
        )
    return day_start, day_end, time_span_str


def format_time_axis(ax, n_hours):
    if n_hours <= 24:
        ax.xaxis.set_major_formatter(mdates.DateFormatter("%H:%M", tz=timezone.utc))
    else:
        ax.xaxis.set_major_formatter(mdates.DateFormatter("%m-%d\n%H:%M", tz=timezone.utc))
    plt.xticks(rotation=45)


def newest_detection_time(dfs, fallback_t):
    if dfs.shape[0] == 0:
        return fallback_t
    finite = n.isfinite(dfs[:, 0])
    if not n.any(finite):
        return fallback_t
    return float(n.nanmax(dfs[finite, 0]))


def frequency_mhz(freq):
    freq = n.asarray(freq, dtype=float)
    finite = n.isfinite(freq)
    if not n.any(finite):
        return freq
    scale = 1e6 if n.nanmedian(n.abs(freq[finite])) > 1e3 else 1.0
    return freq / scale


def plot_chirp_time(dfs, start_t, n_hours=24, min_detections=5,
                    pfname="/tmp/chirp-times.png", station_name="TGO",
                    title_span=None, **_ignored):
    gidx = n.where((dfs[:, 0] > start_t) & (dfs[:, 0] < (start_t+n_hours*3600)))[0]
    dfs = dfs[gidx, :]
    if dfs.shape[0] == 0:
        print("no detections in requested window for %s" % (pfname))
        return

    print("filtering")
    gidx = detection_filter_indices_by_floor_time_and_rate(
        dfs, min_detections=min_detections, max_dt=0.033)
    if len(gidx) == 0:
        print("no soundings with at least %d detections for %s" % (min_detections, pfname))
        return

    freqs = frequency_mhz(dfs[gidx, 2])
    valid = (
        n.isfinite(dfs[gidx, 0]) &
        n.isfinite(freqs) &
        (freqs >= 0.1) &
        (freqs <= 30.0)
    )
    if not n.any(valid):
        print("no valid frequency rows for %s" % (pfname))
        return
    gidx = gidx[valid]
    freqs = freqs[valid]
    times = unix_to_mpl_dates(dfs[gidx, 0])
    chirp_ms = (dfs[gidx, 0] - n.floor(dfs[gidx, 0])) * 1e3

    fig, ax = plt.subplots(1, 1, figsize=(12, 7), constrained_layout=True)
    sc1 = ax.scatter(
        times,
        chirp_ms,
        c=freqs,
        s=0.5,
        alpha=0.6,
        cmap="rainbow",
        vmin=5,
        vmax=25,
    )
    cb1 = plt.colorbar(sc1, ax=ax)
    cb1.set_label("Frequency (MHz)", fontsize=16)
    cb1.ax.tick_params(labelsize=14)

    day_start, day_end, time_span_str = format_time_span(start_t, n_hours, title_span)
    ax.set_xlim(day_start, day_end)
    ax.set_ylim(0, 1000)
    ax.set_ylabel(r"$t_0-\lfloor t_0\rfloor$ (ms)", fontsize=16)
    ax.set_xlabel("Time (UTC)", fontsize=16)
    ax.set_title(
        "Chirp detections -> %s %s, >= %d detections per floor(chirp_time), chirp-rate"
        % (station_name, time_span_str, min_detections),
        fontsize=18,
    )
    ax.tick_params(axis='both', labelsize=14)
    ax.grid(True, alpha=0.25)
    format_time_axis(ax, n_hours)
    plt.savefig(pfname)
    plt.close()
    print("saved %s" % (pfname))


def transmitter_label(band):
    label = band.get("label", band.get("name", "tx"))
    center = band.get("center_km")
    if center is None:
        return label
    return "%s %.0f km" % (label, center)


def plot_auto_propagation_bands(
    ax,
    station_info,
    station_name,
    transmitter_names=None,
    propagation_factor="auto",
    fractional_half_width=0.15,
):
    if transmitter_names is None:
        transmitter_names = ["NIC", "JORN", "ROTHR1", "ROTHR2", "ROTHR3"]
    bands = propagation.auto_propagation_bands(
        station_info,
        station_name,
        transmitter_names,
        propagation_factor=propagation_factor,
        fractional_half_width=fractional_half_width,
    )
    alphas = [0.18, 0.10, 0.26, 0.22, 0.18, 0.14]
    for i, band in enumerate(bands):
        ax.axhspan(
            band["min_km"],
            band["max_km"],
            color="grey",
            alpha=alphas[i % len(alphas)],
            label=transmitter_label(band),
            zorder=0,
        )
    return bands


def plot_propagation_range(
    dfs,
    start_t,
    n_hours=24,
    min_detections=5,
    pfname="/tmp/dets.png",
    station_name="TGO",
    title_span=None,
    station_info=None,
    propagation_range_bands="auto",
    propagation_range_transmitters=None,
    propagation_range_factor="auto",
    propagation_band_fraction=0.15,
):

    gidx=n.where( (dfs[:,0]>start_t) & (dfs[:,0]<(start_t+n_hours*3600)))[0]
    dfs=dfs[gidx,:]
    if dfs.shape[0] == 0:
        print("no detections in requested window for %s" % (pfname))
        return
    
    # filter soundings so that only ones with sufficiently many detections are shown
    print("filtering")
    # less than 10 km separation between points
    gidx = detection_filter_indices(dfs[:, 0], min_detections=min_detections, max_dt=0.033)

    if len(gidx) == 0:
        print("no soundings with more than %d detections for %s" % (min_detections, pfname))
        return
    
    freqs = frequency_mhz(dfs[gidx, 2])
    valid = (
        n.isfinite(dfs[gidx, 0]) &
        n.isfinite(freqs) &
        (freqs >= 0.1) &
        (freqs <= 30.0)
    )
    if not n.any(valid):
        print("no valid frequency rows for %s" % (pfname))
        return
    gidx = gidx[valid]
    freqs = freqs[valid]
    times = unix_to_mpl_dates(dfs[gidx, 0])

    # Compute group delay
    t_grp = dfs[gidx,0] - n.floor(dfs[gidx,0])
    idx=n.where(t_grp>0.5)[0]
    t_grp[idx]=t_grp[idx]-1.0



    fig, ax = plt.subplots(2, 1, figsize=(10, 14), sharex=True, constrained_layout=True)

    # --- TOP PANEL: range vs time (colored by frequency) ---
    sc1 = ax[0].scatter(
        times,
        t_grp * sc.c / 1e3,
        c=freqs,
        s=0.5,
        cmap="rainbow",
        vmin=5,vmax=25
    )
    y_min = -5e3
    y_max = 17.5e3
    cb1 = plt.colorbar(sc1, ax=ax[0])
    cb1.set_label("Frequency (MHz)", fontsize=16)
    
    # Grey bands show expected one-way virtual ranges from transmitter sites.
    # The "auto" path is calibrated from the old TGO Cyprus/JORN manual bands.
    if propagation_range_bands == "auto" and station_info is not None:
        bands = plot_auto_propagation_bands(
            ax[0],
            station_info,
            station_name,
            transmitter_names=propagation_range_transmitters,
            propagation_factor=propagation_range_factor,
            fractional_half_width=propagation_band_fraction,
        )
        if bands:
            y_max = max(y_max, max(band["max_km"] for band in bands) * 1.05)
    else:
        ax[0].axhspan(3900, 5500, color='grey', alpha=0.2, label='Cyprus', zorder=0)
        ax[0].axhspan(13500, 16900,color='grey', alpha=0.1, label='Australia', zorder=0)
        ax[0].axhspan(6.3e3, 10e3, color='grey', alpha=0.3, label='US', zorder=0)
    
    ax[0].set_ylim([y_min, y_max])
    ax[0].set_ylabel("One-way virtual propagation range (km)", fontsize=16)
    # ax[0].set_ylim([0, 42000])
    ax[0].legend(loc="upper right")

    # --- BOTTOM PANEL: frequency vs time (colored by chirp rate) ---
    sc2=ax[1].scatter(
        times,
        freqs,
        c=t_grp * sc.c / 1e3,
        alpha=0.5,
        s=0.5,
        cmap="rainbow",
        vmin=-5e3,
        vmax=max(20e3, y_max),
    )


    cb2=plt.colorbar(sc2, ax=ax[1])
    cb2.set_label("Virtual propagation distance (km)", fontsize=16)
    cb1.ax.tick_params(labelsize=14)
    cb2.ax.tick_params(labelsize=14)

    ax[0].tick_params(axis='both', labelsize=14)
    ax[1].tick_params(axis='both', labelsize=14)

    ax[1].set_ylabel("Frequency (MHz)", fontsize=16)
    ax[1].set_xlabel(f"Time (UTC)", fontsize=16)
    ax[1].set_ylim(0, 25)


    # current time (UTC)
  #  now = datetime.now(timezone.utc)

    # start of current day (00:00:00 UTC)
 #   day_start = now.replace(hour=0, minute=0, second=0, microsecond=0)
    
    # end of current day (next midnight)
#    day_end = day_start + timedelta(days=1)
    day_start, day_end, time_span_str = format_time_span(start_t, n_hours, title_span)
    # apply limits
    ax[0].set_xlim(day_start, day_end)
    ax[1].set_xlim(day_start, day_end)
    
    # label
    ax[0].set_title(f"ROTHR & JORN -> %s {time_span_str}"%(station_name), fontsize=20)
    
#    times = pd.to_datetime(dfs[gidx,0], unit="s", utc=True)
    # --- shared x-axis formatting ---
 #   time_unix = dfs[-1,0]
  #  time_now=time.time()
 #   ax[0].set_xlim(day_start, day_end)
  #  ax[1].set_xlim(day_start, day_end)
    # Label start time
   # start_str = day_start.strftime("%Y-%m-%d %H:%M:%S UTC")
  #  ax[0].set_title("ROTHR & JORN %s"%(day_start))
    
    format_time_axis(ax[1], n_hours)

    fig.align_ylabels(ax)
#    plt.show()
    plt.savefig(pfname)
    plt.close()
    print("saved %s"%(pfname))
#    plt.tight_layout()
#    plt.show()
    
    return


if __name__ == "__main__":
    set_low_priority()
    import argparse
    parser = argparse.ArgumentParser(description="Plot range-time-frequency")
    parser.add_argument(
        "--config",
        type=str,
        default="examples/marieluise/ramfjordmoen_digisonde.ini",
        help="Path to configuration file"
    )
    parser.add_argument(
        "--data-dir",
        type=str,
        default=None,
        help="Directory containing dated cdetections*.h5 subdirectories. Defaults to config output_dir."
    )
    parser.add_argument(
        "--start",
        type=str,
        default=None,
        help="First UTC day to plot, formatted YYYY-MM-DD. Enables one-shot date-range mode."
    )
    parser.add_argument(
        "--end",
        type=str,
        default=None,
        help="End UTC day/datetime. YYYY-MM-DD is inclusive; datetime is exclusive. Defaults to --start day."
    )
    parser.add_argument(
        "--plot-mode",
        choices=("range", "chirp-time"),
        default="range",
        help="Use the normal virtual-range plot or chirp_time fraction on the y-axis."
    )
    parser.add_argument(
        "--min-detections",
        type=int,
        default=5,
        help="Minimum detections per sounding/group."
    )
    parser.add_argument(
        "--output",
        type=str,
        default=None,
        help="Output plot filename. Defaults to /tmp/latest-...png."
    )
    parser.add_argument(
        "--recent-days",
        type=int,
        default=3,
        help="In live mode, only list cdetections files in root and the last N UTC date directories."
    )
    args = parser.parse_args()
    if args.end is not None and args.start is None:
        parser.error("--end requires --start")
    conf = cc.chirp_config(args.config)
    data_dir = args.data_dir or conf.output_dir
    station_name = conf.station_name

    if args.start is not None:
        start_time = parse_utc_datetime(args.start)
        if args.end is None:
            end_time = start_time + timedelta(days=1)
            title_span = start_time.strftime("%Y-%m-%d UTC")
            output_span = start_time.strftime("%Y-%m-%d")
        else:
            end_time = parse_utc_datetime(args.end)
            if len(args.end) == 10:
                end_time = end_time + timedelta(days=1)
            title_span = "%s to %s UTC" % (
                start_time.strftime("%Y-%m-%d %H:%M"),
                end_time.strftime("%Y-%m-%d %H:%M"),
            )
            output_span = "%s_to_%s" % (
                start_time.strftime("%Y-%m-%dT%H%M"),
                end_time.strftime("%Y-%m-%dT%H%M"),
            )
        if end_time <= start_time:
            raise ValueError("--end must be after --start")

        n_hours = (end_time - start_time).total_seconds() / 3600.0
        files = detection_files(
            data_dir,
            station_name=station_name,
            date_dirs=utc_date_dirs(start_time.timestamp(), end_time.timestamp()))
        dfs = read_detection_files(files)
        if args.output is None:
            suffix = "chirp-time" if args.plot_mode == "chirp-time" else "rothr_jorn"
            pfname = "/tmp/latest-%s-%s-%s.png" % (
                suffix, output_span, station_name)
        else:
            pfname = args.output
        plotter = plot_chirp_time if args.plot_mode == "chirp-time" else plot_propagation_range
        plotter(
            dfs,
            start_time.timestamp(),
            n_hours=n_hours,
            min_detections=args.min_detections,
            pfname=pfname,
            station_name=station_name,
            title_span=title_span,
            station_info=conf.station_info,
            propagation_range_bands=conf.propagation_range_bands,
            propagation_range_transmitters=conf.propagation_range_transmitters,
            propagation_range_factor=conf.propagation_range_factor,
            propagation_band_fraction=conf.propagation_band_fraction)
        sys.exit(0)

    while True:
        n_days=2
        n_read=96*n_days+8
        try:
            recent_dirs = [
                (datetime.now(timezone.utc) - timedelta(days=day_offset)).strftime("%Y-%m-%d")
                for day_offset in range(max(1, args.recent_days))
            ]
            files = detection_files(
                data_dir,
                max_files=n_read,
                station_name=station_name,
                date_dirs=recent_dirs)
            dfs = read_detection_files(files)

            import time
            plot_end = newest_detection_time(dfs, time.time())
            t_start = plot_end - n_days*24*3600
            title_span = "last %d hours ending %s UTC" % (
                24*n_days,
                datetime.fromtimestamp(plot_end, timezone.utc).strftime("%Y-%m-%d %H:%M"),
            )

            plotter = plot_chirp_time if args.plot_mode == "chirp-time" else plot_propagation_range
            plotter(
                dfs,
                t_start,
                n_hours=24*n_days,
                min_detections=args.min_detections,
                pfname="/tmp/latest-%s-%s.png" % (
                    "chirp-time" if args.plot_mode == "chirp-time" else "rothr_jorn",
                    station_name),
                station_name=station_name,
                title_span=title_span,
                station_info=conf.station_info,
                propagation_range_bands=conf.propagation_range_bands,
                propagation_range_transmitters=conf.propagation_range_transmitters,
                propagation_range_factor=conf.propagation_range_factor,
                propagation_band_fraction=conf.propagation_band_fraction)
        except Exception:
            traceback.print_exc(file=sys.stdout)
        time.sleep(15*60)
