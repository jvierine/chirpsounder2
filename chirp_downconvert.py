#!/usr/bin/env python
#
# Scan through a digital rf recording
#
import numpy as n
import digital_rf as drf
from mpi4py import MPI
import glob
import fast_exp as fe
import scipy.signal as ss
import scipy.constants as c
import h5py
import chirp_config as cc
import pyfftw
import matplotlib.pyplot as plt
import time
import os

# c library
import chirp_lib as cl

comm=MPI.COMM_WORLD
size=comm.Get_size()
rank=comm.Get_rank()

def fft(z,l=None):
    """
    wrap fft, so that it can be configured
    """
    if l==None:
        l=len(z)
    return(pyfftw.interfaces.numpy_fft.fft(z,l,planner_effort='FFTW_ESTIMATE'))

def ifft(z,l=None):
    """
    wrap fft, so that it can be configured
    """
    if l==None:
        l=len(z)
    return(pyfftw.interfaces.numpy_fft.ifft(z,l,planner_effort='FFTW_ESTIMATE'))        
#    return(sf.ifft(z,l))


def power(z):
    return(z.real**2.0+z.imag**2.0)

def get_m_per_Hz(rate):
    """
    Determine resolution of a sounding.
    """
    # rate = [Hz/s]
    # 1/rate = [s/Hz]
    dt=1.0/rate
    # m/Hz round trip
    return(dt*c.c/2.0)

def chirp(L,f0=-25e3,cr=160e3,sr=50e3,use_numpy=False):
    """
    Generate a chirp.
    """
    tv=n.arange(L,dtype=n.float64)/sr
    dphase=0.5*tv**2*cr*2*n.pi

    if use_numpy:
        chirp=n.exp(1j*n.mod(dphase,2*n.pi))*n.exp(1j*2*n.pi*f0*tv)
    else:
        # table lookup based faster version
        chirp=fe.expf(dphase)*fe.expf((2*n.pi*f0)*tv)
        #   chirp=fe.expf(dphase+(2*n.pi*f0)*tv)#*fe.expf()
    return(chirp)

def spectrogram(x,window=1024,step=512,wf=ss.hann(1024)):
    n_spec=(len(x)-window)/step
    S=n.zeros([n_spec,window])
    for i in range(n_spec):
        S[i,] = n.abs(n.fft.fftshift(n.fft.fft(wf*x[(i*step):(i*step+window)])))**2.0
    return(S)

def decimate(x,dec):
    Nout = int(n.floor(len(x)/dec))
    idx = n.arange(Nout,dtype=n.int)*int(dec)
    res = n.zeros(len(idx),dtype=x.dtype)

    for i in n.arange(dec):
        res += x[idx+i]
    return(res/float(dec))

def chirp_downconvert(conf,
                      t0,
                      d,
                      i0,                  
                      ch,
                      rate,
                      dec=2500):

    sr=conf.sample_rate
    cf=conf.center_freq
    dur=sr/rate
    idx=0
    step=1000
    n_windows=int(dur*sr/(step*dec))+1
    
    cdc=cl.chirp_downconvert(f0=-cf,
                             rate=rate,
                             dec=dec,
                             dt=1.0/conf.sample_rate)

    zd_len=n_windows*step
    zd=n.zeros(zd_len,dtype=n.complex64)

    z_out=n.zeros(step,dtype=n.complex64)
    n_out=step
    for fi in range(n_windows):
        cput0=time.time()
        try:
            z=d.read_vector_c81d(i0+idx,step*dec+cdc.filter_len*dec,ch)
        except:
            z=n.zeros(step*dec+cdc.filter_len*dec,dtype=n.complex64)
        
        cdc.consume(z,z_out,n_out)
#        plt.plot(z_out.real)
 #       plt.plot(z_out.imag)
  #      plt.show()
        zd[(fi*step):(fi*step+step)]=z_out

        cput1=time.time()
        if fi%100==0:
            print("%d/%d speed %1.2f * realtime"%(fi,n_windows, (step*dec/sr)/(cput1-cput0)) )
        
        idx+=dec*step

    dr=conf.range_resolution
    df=conf.frequency_resolution
    sr_dec = sr/dec
    ds=get_m_per_Hz(rate)
    fftlen = int(sr_dec*ds/dr/2.0)*2
    fft_step=int((df/rate)*sr_dec)

    S=spectrogram(n.conj(zd),window=fftlen,step=fft_step,wf=ss.hann(fftlen))

    freqs=rate*n.arange(S.shape[0])*fft_step/sr_dec
    range_gates=ds*n.fft.fftshift(n.fft.fftfreq(fftlen,d=1.0/sr_dec))

    ridx=n.where(n.abs(range_gates) < conf.max_range_extent)[0]
    print(len(ridx))
    try:
        ho=h5py.File("%s/lfm_ionogram-%1.2f.h5"%(conf.output_dir,t0),"w")
        ho["S"]=S[:,ridx]          # ionogram frequency-range
        ho["freqs"]=freqs  # frequency bins
        ho["rate"]=rate    # chirp-rate
        ho["ranges"]=range_gates[ridx]
        ho["t0"]=t0
        ho["sr"]=float(sr_dec) # ionogram sample-rate
        ho["ch"]=ch            # channel name
#        ho["noise_pwr"]=noise_pwr   # noise power for each frequency bin
 #       ho["noise_peak"]=noise_peak # peak noise power for each frequency bin
        ho.close()
    except:
        print("error writing file")
    
        

if __name__ == "__main__":
    conf=cc.chirp_config()
    
    d=drf.DigitalRFReader(conf.data_dir)
    
    fl=glob.glob("%s/par-*.h5"%(conf.output_dir))
    n_ionograms=len(fl)
    # mpi scan through dataset
    for ionogram_idx in range(rank,n_ionograms,size):
        h=h5py.File(fl[ionogram_idx],"r")
        chirp_rate=n.copy(h["chirp_rate"].value)
        t0=n.copy(h["t0"].value)
        i0=long(t0*conf.sample_rate)
        print("calculating i0=%d chirp_rate=%1.2f kHz/s t0=%1.2f"%(i0,chirp_rate/1e3,t0))
        h.close()
        # remove file, because we're now done with it.
#        os.system("rm %s"%(fl[ionogram_idx]))
        chirp_downconvert(conf,
                          t0,
                          d,
                          i0,                  
                          conf.channel,
                          chirp_rate,
                          dec=2500)




    
