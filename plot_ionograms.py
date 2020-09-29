#!/usr/bin/env python

import numpy as n
import matplotlib.pyplot as plt
import glob
import h5py

def plot_ionogram(t0=1.60046428e9,dt=0.02,chirp_rate=100e3):
    fl=glob.glob("%s/*.h5"%(data_dir))
    fnames=[]
    chirp_times=[]
    for f in fl:
        h=h5py.File(f,"r")
        if n.abs(h["chirp_rate"].value - chirp_rate) < 1.0:
            if n.abs(h["chirp_time"].value - t0) < dt:
                fnames.append(f)
                chirp_times.append(h["chirp_time"].value)
        h.close()
    chirp_times=n.array(chirp_times)
    ct0=n.mean(chirp_times)
    print(ct0)
#    print(fnames)
    
    


def scan_for_chirps(data_dir,dt=0.02):
    """
    go through data files and look for unique soundings
    """
    fl=glob.glob("%s/*.h5"%(data_dir))

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
    plt.plot(chirp_rates,chirp_times,".")
    plt.show()
    
    plt.plot(f0,chirp_rates,".")
    plt.show()
    
    crs=n.unique(chirp_rates)
    for c in crs:
        idx=n.where(chirp_rates == c)[0]
        plt.plot(f0[idx]/1e6,chirp_times[idx],".")
        plt.title(c)
        plt.show()
        
    


if __name__ == "__main__":
    data_dir="chirp_out"
    plot_ionogram()
#    scan_for_chirps(data_dir)

    
