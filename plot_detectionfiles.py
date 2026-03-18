import pandas as pd
import h5py
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
import glob
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

labels={100:"US (ROTHR)",125:"Australia (JORN)"}


def plot_propagation_range(dfs, start_t, n_hours=24,min_detections=5, pfname="/tmp/dets.png", station_name="TGO"):

    gidx=n.where( (dfs[:,0]>start_t) & (dfs[:,0]<(start_t+n_hours*3600)))[0]
    dfs=dfs[gidx,:]
    
    gidx=n.array([],dtype=int)

    # filter soundings so that only ones with sufficiently many detections are shown
    rtimes=n.round(dfs[:,0])
    utimes=n.unique(rtimes)
    print("filtering")
    for ut in utimes:
        idx0=n.where(rtimes==ut)[0]# & (n.abs(dfs[:,0]-ut)<0.1) )[0]
        median_time=n.median( dfs[idx0,0] )
        # less than 10 km  separation between points
        idx=n.where( (rtimes==ut)&( n.abs(dfs[:,0]-median_time)<0.033 ) )[0]# & (n.abs(dfs[:,0]-ut)<0.1) )[0]
#        print(ut,len(idx))
        if len(idx)>min_detections:
            gidx=n.concatenate((gidx,idx))
    
    # Convert unix seconds → UTC datetime
    times = pd.to_datetime(dfs[gidx,0], unit="s", utc=True)

    # Time window
    t_end = times.max()
    t_start = t_end - pd.Timedelta(hours=n_hours)

    # Compute group delay
    t_grp = dfs[gidx,0] - n.floor(dfs[gidx,0])
    idx=n.where(t_grp>0.5)[0]
    t_grp[idx]=t_grp[idx]-1.0



    fig, ax = plt.subplots(2, 1, figsize=(10, 7), sharex=True, constrained_layout=True)

    # --- TOP PANEL: range vs time (colored by frequency) ---
    sc1 = ax[0].scatter(
        times,
        t_grp * sc.c / 1e3,
        c=dfs[gidx, 2] / 1e6,
        s=0.5,
        cmap="rainbow",
        vmin=5,vmax=25
    )
    ax[0].set_ylim([-5e3,42e3])
    cb1 = plt.colorbar(sc1, ax=ax[0])
    cb1.set_label("Frequency (MHz)")
    

    
    ax[0].set_ylabel("One-way virtual propagation range (km)")
    # ax[0].set_ylim([0, 42000])


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
    cb2.set_label("Virtual propagation distance (km)")

    ax[1].set_ylabel("Frequency (MHz)")
    ax[1].set_xlabel(f"Time (UTC)")


    # current time (UTC)
  #  now = datetime.now(timezone.utc)

    # start of current day (00:00:00 UTC)
 #   day_start = now.replace(hour=0, minute=0, second=0, microsecond=0)
    
    # end of current day (next midnight)
#    day_end = day_start + timedelta(days=1)
    day_start = datetime.fromtimestamp(start_t)
    day_end = datetime.fromtimestamp(start_t+n_hours*3600)
    # apply limits
    ax[0].set_xlim(day_start, day_end)
    ax[1].set_xlim(day_start, day_end)
    
    # label
    start_str = day_start.strftime("%Y-%m-%d UTC")
    ax[0].set_title(f"ROTHR & JORN -> %s {start_str}"%(station_name))
    
#    times = pd.to_datetime(dfs[gidx,0], unit="s", utc=True)
    # --- shared x-axis formatting ---
 #   time_unix = dfs[-1,0]
  #  time_now=time.time()
 #   ax[0].set_xlim(day_start, day_end)
  #  ax[1].set_xlim(day_start, day_end)
    # Label start time
   # start_str = day_start.strftime("%Y-%m-%d %H:%M:%S UTC")
  #  ax[0].set_title("ROTHR & JORN %s"%(day_start))
    
    ax[1].xaxis.set_major_formatter(mdates.DateFormatter("%H:%M"))
    plt.xticks(rotation=45)

    fig.align_ylabels(ax)
    plt.savefig(pfname)
    plt.close()
    print("saved %s"%(pfname))
#    plt.tight_layout()
#    plt.show()
    
    return


while True:
    if len(sys.argv) == 2:
        conf = cc.chirp_config(sys.argv[1])
    else:
        conf = cc.chirp_config()

    dfs=[]
    files=glob.glob("/data0/2*/cdetections*.h5")
    files.sort()
    
    n_days=2
    n_read=96*n_days+1
    files=files[-n_read:]
    
    #detections.append([chirp_time,i0/25e6,f0,chirp_rate,snr])
    for f in files:
        print(f)
        with h5py.File(f,"r") as h:
            dfs.append(h["data"][()])

    dfs=n.concatenate(dfs,axis=0)


    import time
    tnow=time.time()
    t_day_now=n.floor(tnow/24/3600)*24*3600
    t_day_prev=t_day_now-24*3600
    conf
    plot_propagation_range(dfs, t_day_now, n_hours=24, pfname="/tmp/rothr_jorn_today.png", station_name=conf.station_name)
    plot_propagation_range(dfs, t_day_prev, n_hours=24, pfname="/tmp/rothr_jorn_yesterday.png", station_name=conf.station_name)
    time.sleep(15*60)
