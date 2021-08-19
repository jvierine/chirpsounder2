#!/usr/bin/env python
import matplotlib
matplotlib.use('Agg')
import numpy as n
import matplotlib.pyplot as plt
import glob
import h5py
import scipy.constants as c
import chirp_config as cc
import chirp_det as cd
import sys
import os
import time

def plot_ionogram(conf,f,normalize_by_frequency=True):
    ho=h5py.File(f,"r")
    t0=float(n.copy(ho[("t0")]))
    if not "id" in ho.keys():
        return
    cid=int(n.copy(ho[("id")]))  # ionosonde id
    
    img_fname="%s/%s/lfm_ionogram-%03d-%1.2f.png"%(conf.output_dir,cd.unix2dirname(t0),cid,t0)
    if os.path.exists(img_fname):
        #print("Ionogram plot %s already exists. Skipping"%(img_fname))
        ho.close()
        return
    
    print("Plotting %s rate %1.2f (kHz/s) t0 %1.5f (unix)"%(f,float(n.copy(ho[("rate")]))/1e3,float(n.copy(ho[("t0")]))))
    S=n.copy(ho[("S")])          # ionogram frequency-range
    freqs=n.copy(ho[("freqs")])  # frequency bins
    ranges=n.copy(ho[("ranges")])  # range gates


    if normalize_by_frequency:
        for i in range(S.shape[0]):
            noise=n.nanmedian(S[i,:])
            S[i,:]=(S[i,:]-noise)/noise
        S[S<=0.0]=1e-3
            
    max_range_idx=n.argmax(n.max(S,axis=0))
    
    dB=n.transpose(10.0*n.log10(S))
    if normalize_by_frequency == False:
        dB=dB-n.nanmedian(dB)

    dB[n.isnan(dB)]=0.0
    dB[n.isfinite(dB)!=True]=0.0    
    
    # assume that t0 is at the start of a standard unix second
    # therefore, the propagation time is anything added to a full second

    dt=(t0-n.floor(t0))
    dr=dt*c.c/1e3
    range_gates=dr+2*ranges/1e3
    r0=range_gates[max_range_idx]
    fig=plt.figure(figsize=(1.5*8,1.5*6))
    plt.pcolormesh(freqs/1e6,range_gates,dB,vmin=-3,vmax=30.0,cmap="inferno")
    cb=plt.colorbar()
    cb.set_label("SNR (dB)")
    plt.title("Chirp-rate %1.2f kHz/s t0=%1.5f (unix s)\n%s (UTC)"%(float(n.copy(ho[("rate")]))/1e3,float(n.copy(ho[("t0")])),cd.unix2datestr(float(n.copy(ho[("t0")])))))
    plt.xlabel("Frequency (MHz)")
    plt.ylabel("One-way range offset (km)")
    plt.ylim([dr-conf.max_range_extent/1e3,dr+conf.max_range_extent/1e3])
#    plt.ylim([dr-1000.0,dr+1000.0])
    plt.xlim([0,conf.maximum_analysis_frequency/1e6])
    plt.tight_layout()
    plt.savefig(img_fname)
    fig.clf()
    plt.clf()
    plt.close("all")
    import gc
    gc.collect()
    ho.close()
    sys.stdout.flush()


if __name__ == "__main__":
    if len(sys.argv) == 2:
        conf=cc.chirp_config(sys.argv[1])
    else:
        conf=cc.chirp_config()

    if conf.realtime:
        while True:
            fl=glob.glob("%s/*/lfm*.h5"%(conf.output_dir))
            fl.sort()
            for f in fl:
                plot_ionogram(conf,f)
            time.sleep(10)
    else:
        fl=glob.glob("%s/*/lfm*.h5"%(conf.output_dir))
        for f in fl:
            plot_ionogram(conf,f)

                
            
