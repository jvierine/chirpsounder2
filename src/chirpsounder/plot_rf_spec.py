#!/usr/bin/env python
#
# Plot as quick overview plot of the time-frequency spectrum of the dataset
#
import digital_rf as drf
import numpy as n
import matplotlib.pyplot as plt
import sys
import chirp_config as cc
import scipy.signal as ss
import chirp_det as cd

if __name__ == "__main__":
    n_spec=100
    n_avg=10
    n_fft=4096
    if len(sys.argv) == 2:
        conf=cc.chirp_config(sys.argv[1])
    else:
        conf=cc.chirp_config()
    
    d=drf.DigitalRFReader(conf.data_dir)
    b=d.get_bounds(conf.channel)
    dt=n.floor((b[1]-b[0]-conf.sample_rate)/n_spec)
    wf=n.array(ss.hann(n_fft),dtype=n.float32)
    S=n.zeros([n_fft,n_spec],dtype=n.float32)
    i0=b[0]+conf.sample_rate
    rms_voltage=0.0
    n_rms_voltage=0.0
    tvec=n.zeros(n_spec)
    fvec=n.fft.fftshift(n.fft.fftfreq(n_fft,d=1.0/conf.sample_rate))/1e6 + conf.center_freq/1e6
    for i in range(n_spec):
        print(i)
        for j in range(n_avg):
            try:
                z=d.read_vector_c81d(i0+i*dt+j*n_fft,n_fft,conf.channel)
                rms_voltage+=n.mean(n.abs(z)**2.0)
                n_rms_voltage+=1.0
                S[:,i]+=n.fft.fftshift(n.abs(cd.fft(wf*z))**2.0)
            except:
                print("missing data")
            
        tvec[i]=i*dt/conf.sample_rate

    dB=10.0*n.log10(S)
    dB=dB-n.nanmedian(dB)
    rms_voltage=n.sqrt(rms_voltage/n_rms_voltage)
    plt.pcolormesh(tvec,fvec,dB,vmin=-10,vmax=50.0,cmap="plasma")
    plt.colorbar()
    plt.title("$V_{\mathrm{RMS}}=%1.6f$ (ADC units)"%(rms_voltage))
    plt.xlabel("Time (s)")
    plt.ylabel("Frequency (MHz)")
    plt.ylim([(-conf.sample_rate/2.0/1e6+conf.center_freq/1e6),(conf.sample_rate/2.0/1e6+conf.center_freq/1e6)])
    plt.savefig("./tf_spectrum.pdf")
    
