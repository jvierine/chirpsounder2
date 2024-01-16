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
import chirp_det as cd
import os
import os.path
import time
import pdb

def kill(conf):
    exists = os.path.isfile(conf.kill_path)
    return exists

def cluster_times(t,dt=0.1,dt2=0.02,min_det=2):
    t0s=dt*n.array(n.unique(n.array(n.round(t/dt),dtype=n.int)),dtype=n.float)
    ct0s=[]

    for t0 in t0s:
        tidx=n.where(n.abs(t-t0) < dt)[0]
        if len(tidx) >= min_det:
            ct0s.append(n.mean(t[tidx]))

    t0s=n.unique(ct0s)
    ct0s=[]
    num_dets=[]
    for t0 in t0s:
        tidx=n.where(n.abs(t-t0) < dt2)[0]
        if len(tidx) >= min_det:
            meant=n.mean(t[tidx])
            good=True
            for ct in ct0s:
                if n.abs(meant-ct) < dt: # dupe
                    good=False
            if good:
                ct0s.append(meant)
                num_dets.append(len(tidx))

    return(ct0s,num_dets)
            
def scan_for_chirps(conf,ch,dt=0.1):
    """
    go through data files and look for unique soundings
    """
    data_dir=conf.output_dir
    # detection files have names chirp*.h5

    if conf.realtime:
        this_day_dname="%s/%s"%(conf.output_dir,cd.unix2dirname(time.time()))
        # today
        fl=glob.glob("%s/chirp-%s*.h5"%(this_day_dname,ch))
        fl.sort()
        # latest 100 detections
        if len(fl)>500:
            fl=fl[(len(fl)-500):len(fl)]
        if len(fl) == 0:
            print("no chirp detections yet")
            return
    else:
        # look for all in batch mode
        fl=glob.glob("%s/2*/chirp-%s*.h5"%(data_dir,ch))
        fl.sort()
        
    chirp_rates=[]
    f0=[]    
    chirp_times=[]
    snrs=[]        
    for f in fl:
        try:
            h=h5py.File(f,"r")
            chirp_times.append(n.copy(h[("chirp_time")]))
            chirp_rates.append(n.copy(h[("chirp_rate")]))
            f0.append(n.copy(h[("f0")]))
            if "snr" in h.keys():
                snrs.append(n.copy(h[("snr")]))
            else:
                snrs.append(-1.0)
            h.close()
        except:
            print("Couldn't open %s"%(f))

    chirp_times=n.array(chirp_times)
    chirp_rates=n.array(chirp_rates)
    f0=n.array(f0)
    snrs=n.array(snrs)        


    n_ionograms=0
    crs=n.unique(chirp_rates)
    for c in crs:
        idx=n.where(chirp_rates == c)[0]
        t0s,num_dets=cluster_times(chirp_times[idx],dt,min_det=conf.min_detections)

        for ti,t0 in enumerate(t0s):

            if not conf.realtime:
                print("%s Found chirp-rate %1.2f kHz/s t0=%1.4f num_det %d"%(ch,c/1e3,t0,num_dets[ti]))
                n_ionograms+=1

            if conf.plot_timings:
                plt.axhline(t0,color="red")
            
            dname="%s/%s"%(data_dir,cd.unix2dirname(n.floor(t0)))
            if not os.path.exists(dname):
                os.mkdir(dname)

            fname="%s/par-%s-%1.4f.h5"%(dname,ch,n.floor(t0))
            
            if not os.path.exists(fname):
                ho=h5py.File(fname,"w")
                tnow=time.time()
                t1=(t0+conf.maximum_analysis_frequency/c)
                print("%s Found chirp-rate %1.2f kHz/s t0=%1.4f num_det %d started %1.2f s ago %1.2f s left"%(ch,c/1e3,t0,num_dets[ti],tnow-t0,t1-tnow))
                print("writing file %s"%(fname))
                ho["chirp_rate"]=c
                ho["t0"]=t0
                sweep_idx=n.where( (n.abs(chirp_times-t0)<dt) & (n.abs(chirp_rates-c)<0.1) )[0]
                ho["f0"]=f0[sweep_idx]
                ho["t0s"]=chirp_times[sweep_idx]
                ho["snrs"]=snrs[sweep_idx]            
                ho["channel"]=ch
                ho.close()
                    
        if conf.plot_timings:
            plt.plot(f0[idx]/1e6,chirp_times[idx],".")

        if conf.plot_timings:
            plt.xlabel("Frequency (MHz)")
            plt.ylabel("Time (unix)")
            plt.xlim([0,conf.maximum_analysis_frequency/1e6])
            plt.title("Chirp-rate %1.2f kHz/s"%(c/1e3))
            plt.show()

    if not conf.realtime:
        print("%s Found %d ionograms in total"%(ch,n_ionograms))

if __name__ == "__main__":
    if len(sys.argv) == 3:
        conf=cc.chirp_config(sys.argv[1])
    else:
        print('No config provided - Using defaults')
        conf=cc.chirp_config()

    if conf.realtime:
        print("Scanning for timings indefinitely")
        time.sleep(10)
        sys.stdout.flush()
        while True:
            if kill(conf):
                print("kill.txt found, stopping find_timings.py")
                sys.exit(0)
            else:
                if conf.debug_timings:
                    print("find_timings: scanning for new sounders")
                scan_for_chirps(conf,sys.argv[2])
                if conf.debug_timings:
                    print("find_timings: sleeping 10 seconds")
                time.sleep(1.0)
                sys.stdout.flush()
    else:
        print("Scanning for timings once in batch")
        scan_for_chirps(conf,sys.argv[2])
                

    
