import pandas as pd
import h5py
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
import glob
import numpy as n
import scipy.constants as sc

labels={100:"US (ROTHR)",125:"Australia (JORN)"}

dfs=[]
files=glob.glob("/data0/2*/cdetections*.h5")
files.sort()

n_days=2
n_week=96*n_days
files=files[-n_week:]

#detections.append([chirp_time,i0/25e6,f0,chirp_rate,snr])
for f in files:
    print(f)
    with h5py.File(f,"r") as h:
        dfs.append(h["data"][()])


def plot_propagation_range(dfs, n_hours=12,min_detections=10):

    gidx=n.array([],dtype=int)
    
    rtimes=n.round(dfs[:,0])
    utimes=n.unique(rtimes)
    for ut in utimes:
        idx0=n.where(rtimes==ut)[0]# & (n.abs(dfs[:,0]-ut)<0.1) )[0]
        median_time=n.median( dfs[idx0,0] )
        # less than 10 km  separation between points
        idx=n.where( (rtimes==ut)&( n.abs(dfs[:,0]-median_time)<0.033 ) )[0]# & (n.abs(dfs[:,0]-ut)<0.1) )[0]
        print(ut,len(idx))
        if len(idx)>min_detections:
            gidx=n.concatenate((gidx,idx))
    
        
    
    # Convert unix seconds → UTC datetime
    times = pd.to_datetime(dfs[gidx,0], unit="s", utc=True)

    # Time window
    t_end = times.max()
    t_start = t_end - pd.Timedelta(hours=n_hours)
    print(t_end)
    print(t_start)
    # Compute range
    rgs = dfs[gidx,0] - n.floor(dfs[gidx,0])

    # Label start time
    start_str = t_start.strftime("%Y-%m-%d %H:%M:%S UTC")

    plt.figure(figsize=(10,5))

    plt.scatter(
        times,
        rgs * sc.c / 1e3,
        c=dfs[gidx,2] / 1e6,
        s=1,
        cmap="rainbow"
    )

    cb = plt.colorbar()
    cb.set_label("Frequency (MHz)")

    plt.xlabel(f"Time (UTC) — start: {start_str}")
    plt.ylabel("One-way virtual propagation range (km)")
#    plt.ylim([0,42000])
    plt.xlim([t_start,t_end])
    # Format datetime axis
    ax = plt.gca()
    ax.xaxis.set_major_formatter(mdates.DateFormatter("%H:%M\n%Y-%m-%d"))
    plt.xticks(rotation=45)

    plt.tight_layout()
    plt.show()
        
dfs=n.concatenate(dfs,axis=0)
plot_propagation_range(dfs, n_hours=12)


rgs=dfs[:,0]-n.floor(dfs[:,0])
times=pd.to_datetime(dfs[:,0],unit="s")
crs=n.unique(dfs[:,3])



fig,axs=plt.subplots(len(crs),1,figsize=(13,6),sharex=True)

if len(crs)==1:
    axs=[axs]

#colors=["#00ffff","#ff00ff","#00ff88","#ffaa00"]
colors=["C0","C1"]
for i,cr in enumerate(crs):

    ax=axs[i]

    mask=dfs[:,3]==cr

    ax.plot(
        times[mask],
        dfs[mask,2]/1e6,
        ".",
        color=colors[i%len(colors)],
        alpha=0.25,
        markersize=3
    )

    ax.set_ylabel("MHz")
    ax.set_title(labels[int(cr/1e3)],loc="left")

    ax.grid(True,which="major",alpha=0.5)
    ax.grid(True,which="minor",alpha=0.3,linestyle="--")

# time axis formatting
axs[-1].xaxis.set_major_locator(mdates.DayLocator())
axs[-1].xaxis.set_major_formatter(mdates.DateFormatter("%Y-%m-%d"))
axs[-1].xaxis.set_minor_locator(mdates.HourLocator(byhour=[12]))

plt.setp(axs[-1].get_xticklabels(),rotation=45,ha="right")

tmax=times.max()
tmin=tmax-pd.Timedelta(days=n_days)

axs[-1].set_xlim(tmin,tmax)

axs[-1].set_xlabel("Date (UTC)")

plt.tight_layout()
plt.show()

