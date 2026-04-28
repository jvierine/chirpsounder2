import numpy as n
import glob
import h5py
import os
import matplotlib.pyplot as plt
import chirp_config as cc
import chirp_det as cd
import re
import time
import matplotlib.dates as mdates
from datetime import datetime
import psutil
from datetime import datetime, timedelta

p = psutil.Process()
# Set I/O priority to idle (lowest) to avoid interrupting realtime processes
p.ionice(psutil.IOPRIO_CLASS_IDLE)
p.nice(19)

def needs_daily_plot(pfname, now=None):
    if now is None:
        now = time.time()
    if not os.path.exists(pfname):
        return True
    day_start = n.floor(now/24/3600)*24*3600
    return os.path.getmtime(pfname) < day_start

def read_ionogram(h):
    if "type" in h.keys():
        htype = h["type"][()]
        if hasattr(htype, "decode"):
            htype = htype.decode("utf-8")
        sounder_type = "digisonde" if htype == "digisonde" else "lfm"
    else:
        sounder_type = "lfm"

    if "ranges" in h.keys():
        ranges = h["ranges"][()]
        if len(ranges) > 1 and ranges[1] < 1e2:
            ranges = ranges*1e3
    else:
        ranges = h["rvec"][()]
        if len(ranges) > 1 and ranges[1] < 1e2:
            ranges = ranges*1e3

    if "freqs" in h.keys():
        freqs = h["freqs"][()]
    else:
        freqs = h["fvec"][()]

    snr = h["SNR"][()]
    if sounder_type == "digisonde":
        snr = snr[0, :, :]

    n_f = min(len(freqs), snr.shape[0])
    n_r = min(len(ranges), snr.shape[1])
    return ranges[:n_r], freqs[:n_f], snr[:n_f, :n_r], h["t0"][()]

def get_day_view(conf,tx,rx,dirname,pfname="/tmp/latest-rti.png"):
    print("creating RTI and RTF")
    fl=glob.glob("%s/%s/*_ionogram-%s-%s-*.h5"%(conf.output_dir,dirname,tx,rx))
    fl.sort()
    if len(fl)<3:
        print("not enough soundings %s %s"%(tx,rx))
        return

    with h5py.File(fl[-1], "r") as h:
        ranges, freqs, snr, t0 = read_ionogram(h)
    range_keys = n.array(n.round(ranges), dtype=n.int64)
    if len(range_keys) == 0:
        print("no range gates %s %s"%(tx,rx))
        return
    ranges = n.array(ranges, dtype=n.float64)
    range_to_idx = {rk: ri for ri, rk in enumerate(range_keys)}
    n_r = len(ranges)

    n_t=len(fl)
    S=n.full([n_t,n_r], n.nan)
    M=n.full([n_t,n_r], n.nan)
    tv=n.zeros(n_t)
    for fi,f in enumerate(fl):
        with h5py.File(f, "r") as h:
            cur_ranges, cur_freqs, SNR, tv[fi] = read_ionogram(h)
        cur_n_r = SNR.shape[1]
        cur_n_f = SNR.shape[0]
        cur_keys = n.array(n.round(cur_ranges[:cur_n_r]), dtype=n.int64)
        for ri in range(cur_n_r):
            out_ri = range_to_idx.get(cur_keys[ri])
            if out_ri is None:
                continue
            col = SNR[:cur_n_f, ri]

            if n.all(n.isnan(col)):
                # case: all NaN → set outputs to NaN
                S[fi, out_ri] = n.nan
                M[fi, out_ri] = n.nan
            else:
                # normal case
                M[fi, out_ri] = n.nanmax(col)
                S[fi, out_ri] = cur_freqs[:cur_n_f][n.nanargmax(col)]
                
#            S[fi,ri]=freqs[n.nanargmax(SNR[:,ri])]
 #           M[fi,ri]=n.nanmax(SNR[:,ri])


    # convert unix time to datetime
    t = n.array([datetime.utcfromtimestamp(x) for x in tv])

    # --- detect time gaps ---
    dt = n.diff(tv)
    gap_threshold = 2*n.median(dt)

    t_new = [t[0]]
    M_new = [M[0]]
    S_new = [S[0]]

    for i in range(1, len(tv)):
        if tv[i] - tv[i-1] > gap_threshold:
            # insert gap column
            t_new.append(t[i-1])
            M_new.append(n.full_like(M[i], n.nan))
            S_new.append(n.full_like(S[i], n.nan))
        t_new.append(t[i])
        M_new.append(M[i])
        S_new.append(S[i])

    t_new = n.array(t_new)
    M_new = n.array(M_new)
    S_new = n.array(S_new)

    fig, ax = plt.subplots(2,1,figsize=(10,6),sharex=True)
    
    # --- first plot ---
    pcm1 = ax[0].pcolormesh(
        t_new,
        ranges/1e3,
        10*n.log10(M_new.T),
        shading="auto",
        cmap="gist_yarg",
        vmin=3,
        vmax=20
    )
    
    ax[0].set_ylabel("Propagation virtual range (km)")
    cb1 = plt.colorbar(pcm1, ax=ax[0])
    cb1.set_label("SNR (dB)")

    
    # --- second plot ---
    S_new[M_new<20]=n.nan
    pcm2 = ax[1].pcolormesh(
        t_new,
        ranges/1e3,
        S_new.T/1e6,
        cmap="rainbow",
        shading="auto"
    )
    ax[1].set_ylabel("Propagation virtual range (km)")
    cb2 = plt.colorbar(pcm2, ax=ax[1])
    cb2.set_label("Frequency (MHz)")

    # --- time formatting ---
    ax[1].xaxis.set_major_formatter(mdates.DateFormatter("%H:%M"))
    ax[1].xaxis.set_major_locator(mdates.AutoDateLocator())

    start_date = datetime.utcfromtimestamp(tv[0]).strftime("%Y-%m-%d")
    ax[0].set_title("%s-%s %s"%(tx,rx,start_date))#Date: {start_date}
    ax[1].set_xlabel(f"Time (UTC)")

    # full day
    day_start = t_new.min().replace(hour=0, minute=0, second=0, microsecond=0)
    day_end = day_start + timedelta(days=1)
    ax[0].set_xlim(day_start, day_end)
    ax[1].set_xlim(day_start, day_end)

    plt.tight_layout()
    plt.savefig(pfname)
    plt.close()
#    plt.show()

def plot_rtf(conf,tx,rx):
    tnow=time.time()
    tyesterday=tnow-24*3600
    today_dir=cd.unix2dirname(tnow)
    yesterday_dir=cd.unix2dirname(tyesterday)
    yesterday_pfname="/tmp/yesterday-rti-%s-%s.png"%(tx,rx)
    
    get_day_view(conf,tx,rx,today_dir,pfname="/tmp/latest-rti-%s-%s.png"%(tx,rx))
    if needs_daily_plot(yesterday_pfname, now=tnow):
        get_day_view(conf,tx,rx,yesterday_dir,pfname=yesterday_pfname)
    else:
        print("skipping up-to-date %s"%(yesterday_pfname))

def plot_rtf_links(conf, links):
    for tx, rx in links:
        print("plotting RTF %s -> %s"%(tx, rx))
        plot_rtf(conf, tx, rx)

def normalize_links(links):
    normalized = []
    for link in links:
        if isinstance(link, str):
            parts = link.split(",")
        else:
            parts = list(link)
        if len(parts) != 2:
            print("skipping invalid RTF link %s"%(link))
            continue
        normalized.append([parts[0], parts[1]])
    return normalized

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
        "--sounding_path",
        type=str,
        default="",
        help="Fallback single sounding path, e.g. SGO,TGO. [rtf] links from config take precedence."
    )
    args = parser.parse_args()
    conf = cc.chirp_config(args.config, read_shared=False)
    if len(conf.rtf_links) > 0:
        links = normalize_links(conf.rtf_links)
        print("using [rtf] links from config")
    elif args.sounding_path != "":
        links = normalize_links([args.sounding_path])
        print("using fallback --sounding_path")
    else:
        links = []
    if len(links) == 0:
        print("no RTF links configured")
        exit(0)
    print("RTF links: %s"%(links))
    import time
    plot_period_s = 15*60
    next_plot_time = 0.0
    
    while True:
        now = time.time()
        if now >= next_plot_time:
            plot_rtf_links(conf, links)
            next_plot_time = time.time() + plot_period_s
        time.sleep(max(1.0, min(60.0, next_plot_time - time.time())))
