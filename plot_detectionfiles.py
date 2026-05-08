import pandas as pd
import h5py
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
import glob
import os
import numpy as n
import scipy.constants as sc
import psutil
#from datetime import datetime, timedelta
from datetime import datetime, timedelta, timezone
import chirp_config as cc
import sys
#plt.style.use('dark_background')
p = psutil.Process()
# Set I/O priority to idle (lowest) to avoid interrupting realtime processes
p.ionice(psutil.IOPRIO_CLASS_IDLE)
p.nice(19)

labels={100:"US (ROTHR)",125:"Australia (JORN)"}


def parse_utc_day(day):
    return datetime.strptime(day, "%Y-%m-%d").replace(tzinfo=timezone.utc)


def detection_files(data_dir, max_files=None):
    files = glob.glob("%s/2*/cdetections*.h5" % (data_dir))
    files.sort()
    if max_files is not None:
        files = files[-max_files:]
    return files


def read_detection_files(files):
    dfs = []
    for f in files:
        print(f)
        with h5py.File(f, "r") as h:
            dfs.append(h["data"][()])
    if len(dfs) == 0:
        return n.empty((0, 4))
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


def needs_daily_plot(pfname, now=None):
    if now is None:
        import time
        now = time.time()
    if not os.path.exists(pfname):
        return True
    day_start = n.floor(now/24/3600)*24*3600
    return os.path.getmtime(pfname) < day_start


def plot_propagation_range(dfs, start_t, n_hours=24,min_detections=5, pfname="/tmp/dets.png", station_name="TGO", title_span=None):

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
    
    # Convert unix seconds → UTC datetime
    times = pd.to_datetime(dfs[gidx,0], unit="s", utc=True)

    # Time window
    t_end = times.max()
    t_start = t_end - pd.Timedelta(hours=n_hours)

    # Compute group delay
    t_grp = dfs[gidx,0] - n.floor(dfs[gidx,0])
    idx=n.where(t_grp>0.5)[0]
    t_grp[idx]=t_grp[idx]-1.0



    fig, ax = plt.subplots(2, 1, figsize=(10, 14), sharex=True, constrained_layout=True)

    # --- TOP PANEL: range vs time (colored by frequency) ---
    sc1 = ax[0].scatter(
        times,
        t_grp * sc.c / 1e3,
        c=dfs[gidx, 2] / 1e6,
        s=0.5,
        cmap="rainbow",
        vmin=5,vmax=25
    )
    ax[0].set_ylim([-5e3,17.5e3])
    cb1 = plt.colorbar(sc1, ax=ax[0])
    cb1.set_label("Frequency (MHz)", fontsize=16)
    
    # grey band
    ax[0].axhspan(3900, 5500, color='grey', alpha=0.2, label='Cyprus', zorder=0)
    ax[0].axhspan(13500, 16900,color='grey', alpha=0.1, label='Australia', zorder=0)
    ax[0].axhspan(6.3e3, 10e3, color='grey', alpha=0.3, label='US', zorder=0)    
    
    ax[0].set_ylabel("One-way virtual propagation range (km)", fontsize=16)
    # ax[0].set_ylim([0, 42000])
    ax[0].legend(loc="upper right")

    crs=n.array(dfs[gidx,3]/1e3,dtype=int)
    freqs=dfs[gidx,2]/1e6

    # --- BOTTOM PANEL: frequency vs time (colored by chirp rate) ---
    sc2=ax[1].scatter(
        times,
        freqs,
        c=t_grp * sc.c / 1e3,
        alpha=0.5,
        s=0.5,
        cmap="rainbow",
        vmin=-5e3,
        vmax=20e3,
    )


    cb2=plt.colorbar(sc2, ax=ax[1])
    cb2.set_label("Virtual propagation distance (km)", fontsize=16)
    cb1.ax.tick_params(labelsize=14)
    cb2.ax.tick_params(labelsize=14)

    ax[0].tick_params(axis='both', labelsize=14)
    ax[1].tick_params(axis='both', labelsize=14)

    ax[1].set_ylabel("Frequency (MHz)", fontsize=16)
    ax[1].set_xlabel(f"Time (UTC)", fontsize=16)


    # current time (UTC)
  #  now = datetime.now(timezone.utc)

    # start of current day (00:00:00 UTC)
 #   day_start = now.replace(hour=0, minute=0, second=0, microsecond=0)
    
    # end of current day (next midnight)
#    day_end = day_start + timedelta(days=1)
    day_start = datetime.fromtimestamp(start_t, timezone.utc)
    day_end = datetime.fromtimestamp(start_t+n_hours*3600, timezone.utc)
    # apply limits
    ax[0].set_xlim(day_start, day_end)
    ax[1].set_xlim(day_start, day_end)
    
    # label
    if title_span is not None:
        time_span_str = title_span
    elif n_hours <= 24:
        time_span_str = day_start.strftime("%Y-%m-%d UTC")
    else:
        time_span_str = "%s to %s UTC" % (
            day_start.strftime("%Y-%m-%d"),
            day_end.strftime("%Y-%m-%d"),
        )
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
    
    if n_hours <= 24:
        ax[1].xaxis.set_major_formatter(mdates.DateFormatter("%H:%M"))
    else:
        ax[1].xaxis.set_major_formatter(mdates.DateFormatter("%m-%d\n%H:%M"))
    plt.xticks(rotation=45)

    fig.align_ylabels(ax)
#    plt.show()
    plt.savefig(pfname)
    plt.close()
    print("saved %s"%(pfname))
#    plt.tight_layout()
#    plt.show()
    
    return


if __name__ == "__main__":
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
        help="Last UTC day to include in the same plot, formatted YYYY-MM-DD. Inclusive. Defaults to --start."
    )
    args = parser.parse_args()
    if args.end is not None and args.start is None:
        parser.error("--end requires --start")
    conf = cc.chirp_config(args.config)
    data_dir = args.data_dir or conf.output_dir
    station_name = conf.station_name

    if args.start is not None:
        start_day = parse_utc_day(args.start)
        end_day = parse_utc_day(args.end or args.start)
        if end_day < start_day:
            raise ValueError("--end must be on or after --start")

        end_exclusive = end_day + timedelta(days=1)
        n_hours = (end_exclusive - start_day).total_seconds() / 3600.0
        files = detection_files(data_dir)
        dfs = read_detection_files(files)
        plot_propagation_range(
            dfs,
            start_day.timestamp(),
            n_hours=n_hours,
            pfname="/tmp/latest-rothr_jorn-%s_to_%s-%s.png" % (
                start_day.strftime("%Y-%m-%d"),
                end_day.strftime("%Y-%m-%d"),
                station_name,
            ),
            station_name=station_name,
            title_span="%s to %s UTC" % (
                start_day.strftime("%Y-%m-%d"),
                end_day.strftime("%Y-%m-%d"),
            ))
        sys.exit(0)

    while True:
        n_days=2
        n_read=96*n_days+1
        files = detection_files(data_dir, max_files=n_read)
        dfs = read_detection_files(files)


        import time
        tnow=time.time()
        t_start=tnow - n_days*24*3600

        plot_propagation_range(
            dfs,
            t_start,
            n_hours=24*n_days,
            pfname="/tmp/latest-rothr_jorn-%s.png" % (station_name),
            station_name=station_name)
        time.sleep(15*60)
