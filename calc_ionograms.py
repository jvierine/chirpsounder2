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
    return(chirp)

def decimate(x,dec):
    Nout = int(n.floor(len(x)/dec))
    idx = n.arange(Nout,dtype=n.int)*int(dec)
    res = n.zeros(len(idx),dtype=x.dtype)

    for i in n.arange(dec):
        res += x[idx+i]
    return(res/float(dec))

def analyze_chirp(conf,
                  t0,
                  d,
                  i0,                  
                  ch,
                  rate,
                  dec=2500):

    sr=conf.sample_rate
    cf=conf.center_freq
    
    # todo. they probably should be chirp-rate dependent
    dr=conf.range_resolution
    df=conf.frequency_resolution
    
    sr_dec=sr/dec
    ds=get_m_per_Hz(rate)
    fftlen = int(sr_dec*ds/dr/2.0)*2
    range_gates=ds*n.fft.fftshift(n.fft.fftfreq(fftlen,d=dec/sr))
    dur=conf.maximum_analysis_frequency/rate
    overlap = df*sr/(rate*fftlen*dec)
    df = sr_dec/fftlen
    w=ss.hann(fftlen)
    n_windows=int(dur*sr_dec/fftlen/overlap)
    n_ranges=fftlen
    S=n.zeros([n_windows,n_ranges])
    noise_pwr=n.zeros(n_windows)
    noise_peak=n.zeros(n_windows)    
    t_step=float(float(fftlen*dec*overlap)/float(sr))
    freqs=n.arange(n_windows,dtype=n.float64)*t_step*rate/1e6
    idx=0
    f00=-cf # start at 0 frequency

    for fi in range(n_windows):
        cput0=time.time()
        try:
            z=d.read_vector_c81d(i0+idx,dec*fftlen,ch)
        except:
            print("no data")
            z=n.zeros(dec*fftlen,dtype=n.complex64)

        f0=f00 + rate*idx/sr
        # this part needs to be implemented on a GPU or with C
        # it is _slow_
        dechirp=n.conj(chirp(fftlen*dec,f0=f0,cr=rate,sr=sr))
        zd=ss.decimate(dechirp*z,ftype="fir",q=dec)
        
        noise_peak_est=n.max(power(zd))
        noise_pwr_est=n.median(power(zd))
        Z=n.fft.fftshift(power(fft(w*zd)))[::-1]
        S[fi,:]=Z
        noise_peak[fi]=noise_peak_est
        noise_pwr[fi]=noise_pwr_est
        cput1=time.time()

        analysis_time_step = float(dec*fftlen*overlap)/sr
        
        print("rank %03d. %d %s %04d/%04d rate=%1.0f analysis speed %1.4f * realtime"%(comm.rank,i0+idx,ch,fi,n_windows,rate/1e3,analysis_time_step/(cput1-cput0)))
        
        idx+=int(dec*fftlen*overlap)
        
    nfloor=n.median(10.0*n.log10(S))
    
    try:
        ho=h5py.File("%s/lfm_ionogram-%1.2f.h5"%(conf.output_dir,t0),"w")
        ho["S"]=S          # ionogram frequency-range
        ho["freqs"]=freqs  # frequency bins
        ho["rate"]=rate    # chirp-rate
        ho["ranges"]=range_gates
        ho["t0"]=t0
        ho["sr"]=float(sr_dec) # ionogram sample-rate
        ho["ch"]=ch            # channel name
        ho["noise_pwr"]=noise_pwr   # noise power for each frequency bin
        ho["noise_peak"]=noise_peak # peak noise power for each frequency bin
        ho.close()
    except:
        print("error writing file")

if __name__ == "__main__":
    conf=cc.chirp_config()
    
    d=drf.DigitalRFReader(conf.data_dir)
    # todo: some kind of pooling is needed for a realtime process
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
        # os.system("rm %s"%(fl[ionogram_idx]))
        analyze_chirp(conf,
                      t0,
                      d,
                      i0,                  
                      conf.channel,
                      chirp_rate,
                      dec=2500)




    
