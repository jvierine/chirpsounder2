#!/usr/bin/env python
#
# given predetections, find chirp timings
#
import numpy as n
import matplotlib.pyplot as plt
import glob
import h5py


def cluster_times(t,dt=0.1,dt2=0.02,min_det=10):
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
            
def scan_for_chirps(data_dir,dt=0.1):
    """
    go through data files and look for unique soundings
    """
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
        idx=n.where(chirp_rates == c)[0]
        t0s,num_dets=cluster_times(chirp_times[idx],dt)
 #       plt.plot(f0[idx],chirp_times[idx],".")
        
        for ti,t0 in enumerate(t0s):
  #          plt.axhline(t0,color="red")
            print("Found chirp-rate %1.2f kHz/s t0=%1.4f num_det %d"%(c/1e3,t0,num_dets[ti]))
            #        plt.show()
            ho=h5py.File("%s/par-%1.2f-%1.2f.h5"%(data_dir,c/1e3,t0),"w")
            ho["chirp_rate"]=c
            ho["t0"]=t0
            ho.close()
       


if __name__ == "__main__":
    data_dir="chirp_out"
    scan_for_chirps(data_dir)

    
