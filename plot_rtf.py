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

def get_day_view(conf,tx,rx,dirname,sounder_type="lfm",pfname="/tmp/latest-rti.png"):
    print("creating RTI and RTF")
    fl=glob.glob("%s/%s/%s_ionogram-%s-%s-*.h5"%(conf.output_dir,dirname,sounder_type,tx,rx))
    fl.sort()
    if len(fl)<3:
        print("not enough soundings %s %s %s"%(sounder_type,tx,rx))
        return
    h=h5py.File(fl[0],"r")
    if "ranges" in h.keys():
        ranges=h["ranges"][()]
        if ranges[1]<1e2:
            ranges=ranges*1e3        
    else:
        ranges=h["rvec"][()]
        # km -> m
        if ranges[1]<1e2:
            ranges=ranges*1e3
            
    if "freqs" in h.keys():        
        freqs=h["freqs"][()]
    else:
        freqs=h["fvec"][()]
        
    SNR=h["SNR"][()]

    if sounder_type == "digisonde":
        n_r=SNR.shape[2]
        n_f=SNR.shape[1]
        # O-mode only
        SNR=SNR[0,:,:]
    else:
        n_r=SNR.shape[1]
        n_f=SNR.shape[0]

    n_t=len(fl)
    S=n.zeros([n_t,n_r])
    M=n.zeros([n_t,n_r])
    tv=n.zeros(n_t)
    for fi,f in enumerate(fl):
#        print(f)
        h=h5py.File(f,"r")
#        print(h.keys())
        SNR=h["SNR"][()]
#        print(SNR.shape)
        if sounder_type == "digisonde":
            SNR=SNR[0,:,:]
        tv[fi]=h["t0"][()]
        for ri in range(n_r):
            col = SNR[:, ri]

            if n.all(n.isnan(col)):
                # case: all NaN → set outputs to NaN
                S[fi, ri] = n.nan
                M[fi, ri] = n.nan
            else:
                # normal case
                M[fi, ri] = n.nanmax(col)
                S[fi, ri] = freqs[n.nanargmax(col)]
                
#            S[fi,ri]=freqs[n.nanargmax(SNR[:,ri])]
 #           M[fi,ri]=n.nanmax(SNR[:,ri])
        h.close()


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

def plot_rtf(conf):
    tnow=time.time()
    tyesterday=tnow-24*3600
    today_dir=cd.unix2dirname(tnow)
    yesterday_dir=cd.unix2dirname(tyesterday)
    
    fl=glob.glob("%s/%s/*_ionogram-*.h5"%(conf.output_dir,today_dir))
    fl.sort()

    types=[]
    txs=[]
    rxs=[]
    for f in fl:
#        print(f)
        match=re.search(".*/([^_]+)_ionogram-([^-]+)-([^-]+)-.*.h5",f)
        if len(match.groups())==3:
            iono_type=match.group(1)
            tx=match.group(2)
            rx=match.group(3)
            txs.append(tx)
            rxs.append(rx)
            types.append(iono_type)
#            print(iono_type)
#            print(tx)
 #           print(rx)
    txs=n.unique(txs)
    rxs=n.unique(rxs)
    iono_types=n.unique(types)    
  #  print(txs)
  #  print(rxs)
  #  print(iono_types)

    for tx in txs:
        for rx in rxs:
            for iono_type in iono_types:
                get_day_view(conf,tx,rx,today_dir,iono_type,pfname="/tmp/latest-rti-%s-%s.png"%(tx,rx))
                get_day_view(conf,tx,rx,yesterday_dir,iono_type,pfname="/tmp/yesterday-rti-%s-%s.png"%(tx,rx))  

if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser(description="Plot range-time-frequency")
    parser.add_argument(
        "--config",
        type=str,
        default="examples/marieluise/ramfjordmoen_digisonde.ini",
        help="Path to configuration file"
    )
    args = parser.parse_args()
    conf = cc.chirp_config(args.config)
    import time
    
    while True:
        plot_rtf(conf)
        time.sleep(15*60)
