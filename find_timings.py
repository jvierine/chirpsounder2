#!/usr/bin/env python
#
# given predetections, find chirp timings
#
import numpy as n
import matplotlib.pyplot as plt
import glob
import h5py
import chirp_config as cc
import sys

# set to False if you want to disable
plot=True

def cluster_times(t,dt=0.1,dt2=0.02,min_det=3):
    t0s=dt*n.array(n.unique(n.array(n.round(t/dt),dtype=n.int)),dtype=n.float)
    ct0s=[]
    num_dets=[]

    for t0 in t0s:
        tidx=n.where(n.abs(t-t0) < dt)[0]
        if len(tidx) > min_det:
            ct0s.append(n.mean(t[tidx]))
#            num_dets.append(len(tidx))
    t0s=n.unique(ct0s)
    ct0s=[]
    num_dets=[]
    for t0 in t0s:
        tidx=n.where(n.abs(t-t0) < dt2)[0]
        if len(tidx) > min_det:
            meant=n.mean(t[tidx])
            good=True
            for ct in ct0s:
                if n.abs(meant-ct) < dt: # dupe
                    good=False
            if good:
                ct0s.append(meant)
                num_dets.append(len(tidx))

    return(ct0s,num_dets)
            
def scan_for_chirps(conf,dt=0.1):
    """
    go through data files and look for unique soundings
    """
    data_dir=conf.output_dir
    # detection files have names chirp*.h5
    fl=glob.glob("%s/chirp*.h5"%(data_dir))

    chirp_rates=[]
    f0=[]    
    chirp_times=[]    
    for f in fl:
        h=h5py.File(f,"r")
        chirp_times.append(h["chirp_time"].value)
        chirp_rates.append(h["chirp_rate"].value)
        f0.append(h["f0"].value)                
        h.close()

    chirp_times=n.array(chirp_times)
    chirp_rates=n.array(chirp_rates)
    f0=n.array(f0)    
    
    crs=n.unique(chirp_rates)
    for c in crs:
#        print(c)
        idx=n.where(chirp_rates == c)[0]
        t0s,num_dets=cluster_times(chirp_times[idx],dt)
        tt0=n.min(chirp_times[idx])
        if conf.plot_timings:
            plt.plot(f0[idx]/1e6,chirp_times[idx],".")
        
        for ti,t0 in enumerate(t0s):
            if conf.plot_timings:
                plt.axhline(t0,color="red")
            print("Found chirp-rate %1.2f kHz/s t0=%1.4f num_det %d"%(c/1e3,t0,num_dets[ti]))
            #        plt.show()
            ho=h5py.File("%s/par-%1.4f.h5"%(data_dir,t0),"w")
            ho["chirp_rate"]=c
            ho["t0"]=t0
            ho.close()
        if conf.plot_timings:
            plt.xlabel("Frequency (MHz)")
            plt.ylabel("Time (unix)")
            plt.xlim([0,conf.maximum_analysis_frequency/1e6])
            plt.title("Chirp-rate %1.2f kHz/s"%(c/1e3))
            plt.show()
       


if __name__ == "__main__":
    if len(sys.argv) == 2:
        conf=cc.chirp_config(sys.argv[1])
    else:
        conf=cc.chirp_config()
    scan_for_chirps(conf)

    
