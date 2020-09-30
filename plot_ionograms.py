#!/usr/bin/env python

import numpy as n
import matplotlib.pyplot as plt
import glob
import h5py
import scipy.constants as c
import chirp_config as cc

def plot_ionogram(conf,f):
    ho=h5py.File(f,"r")
    S=ho["S"].value          # ionogram frequency-range
    freqs=ho["freqs"].value  # frequency bins
    ranges=ho["ranges"].value  # range gates
    noise_pwr=ho["noise_pwr"].value
    for i in range(S.shape[0]):
        noise=n.median(n.abs(S[i,:]))
        S[i,:]=(S[i,:]-noise)/noise
    S[S<0]=1e-3
    dB=n.transpose(10.0*n.log10(S))
    
    print("Plotting %s rate %1.2f (kHz/s) t0 %1.5f (unix)"%(f,ho["rate"].value/1e3,ho["t0"].value))
    
    # assume that t0 is at the start of a standard unix second
    # therefore, the propagation time is anything added to a full second
    t0=ho["t0"].value
    dt=(t0-n.floor(t0))
    dr=dt*c.c/1e3
    plt.figure(figsize=(1.5*8,1.5*6))
    plt.pcolormesh(freqs,dr+2*ranges/1e3,dB,vmin=0,vmax=20.0)
    cb=plt.colorbar()
    cb.set_label("SNR (dB)")
    plt.title("Chirp-rate %1.2f kHz/s t0=%1.5f (unix s)"%(ho["rate"].value/1e3,ho["t0"].value))
    plt.xlabel("Frequency (MHz)")
    plt.ylabel("One-way range offset (km)")
    plt.ylim([-2000+dr,2000+dr])
    plt.tight_layout()
    plt.savefig("%s/lfm_ionogram-%1.2f.png"%(conf.output_dir,t0))
    plt.close()
    plt.clf()
    ho.close()


if __name__ == "__main__":
    conf=cc.chirp_config()
    fl=glob.glob("%s/lfm*.h5"%(conf.output_dir))
    for f in fl:
        plot_ionogram(conf,f)
    
