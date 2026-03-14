import pandas as pd
import h5py
import matplotlib.pyplot as plt
import matplotlib.dates as mdates
import glob
import numpy as n

labels={100:"US (ROTHR)",125:"Australia (JORN)"}

dfs=[]
files=glob.glob("/data0/2*/cdetections*.h5")
files.sort()

n_days=2
n_week=96*n_days
files=files[-n_week:]

for f in files:
    print(f)
    with h5py.File(f,"r") as h:
        dfs.append(h["data"][()])

dfs=n.concatenate(dfs,axis=0)

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

