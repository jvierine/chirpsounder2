#!/usr/bin/env python
#
# data format agnostic generic chirp detector
# juha vierinen 2020
#
import numpy as n
import argparse
import scipy.signal as ss
import matplotlib.pyplot as plt
import time
import glob
import re
import os
import scipy.fftpack
fftw=False
try:
    import pyfftw
    fftw=True
    print("using pyfftw")
except:
    print("couldn't load pyfftw, reverting to scipy. performance will suffer")
    fftw=False
    
import h5py
import scipy.constants as c
import datetime

def power(x):
    return(x.real**2.0 + x.imag**2.0)

def fft(x):
    if fftw:
        return(pyfftw.interfaces.numpy_fft.fft(x))#,planner_effort='FFTW_ESTIMATE'))
    else:
        return(scipy.fftpack.fft(x))

def ifft(x):
    if fftw:
        return(pyfftw.interfaces.numpy_fft.ifft(x))#,planner_effort='FFTW_ESTIMATE'))
    else:
        return(scipy.fftpack.ifft(x))

debug_out0=False
def debug0(msg):
    if debug_out0:
        print(msg)
debug_out1=True
def debug1(msg):
    if debug_out1:
        print(msg)

def unix2date(x):
    return datetime.datetime.utcfromtimestamp(n.float(x))  # ATC bug-fix for python3

def unix2datestr(x):
    return(unix2date(x).strftime('%Y-%m-%d %H:%M:%S'))

def unix2dirname(x):
    return(unix2date(x).strftime('%Y-%m-%d'))

class chirp_matched_filter_bank:
    def __init__(self,conf):
        self.conf=conf

        # create chirp signal vectors
        # centered around zero frequency
        self.chirps=[]
        self.wf=n.array(ss.hann(self.conf.n_samples_per_block),dtype=n.float32)
        for cr in self.conf.chirp_rates:
            print("creating filter with chirp-rate %1.2f kHz/s"%(cr/1e3))
            chirp_vec=n.array(self.wf*n.conj(self.chirpf(cr=cr)))
            self.chirps.append(chirp_vec)
        self.n_chirps=len(self.chirps)

    def chirpf(self,cr=160e3):
        """
        Generate a chirp. This is used for matched filtering
        """
        L=self.conf.n_samples_per_block
        sr=self.conf.sample_rate
        f0=0.0
        tv=n.arange(L,dtype=n.float64)/float(sr)
        dphase=0.5*tv**2*cr*2*n.pi
        chirp=n.exp(1j*n.mod(dphase,2*n.pi))*n.exp(1j*2*n.pi*f0*tv)
        return(n.array(chirp,dtype=n.complex64))

    def seek(self,z,i0):
        """
        Look for chirps in data vector
        z data vector
        i0 time of the leading edge of the vector
        """
        cput0=time.time()
        n_samps=len(z)
        
        t0=i0/self.conf.sample_rate
        
        if n_samps != self.conf.n_samples_per_block:
            print("wrong number of samples given to matched filter")
            exit(0)
        
        # whiten noise with a regularized filter
        Z=fft(self.wf*z)
        z=ifft(Z/(n.abs(Z)+1e-9))
        
        # matched filter output
        # store the best matching chirp-rate and
        # normalized SNR (we pre-whiten the signal)
        mf_p = n.zeros(n_samps,dtype=n.float32)
        mf_chirp_rate_idx = n.zeros(n_samps,dtype=n.int32)

        # filter output for all chirps, for storing ionograms
        mf = n.zeros([self.n_chirps,n_samps],dtype=n.float32)
        
        for cri in range(self.n_chirps):
            mf[cri,:]=power(n.fft.fftshift(fft(self.wf*self.chirps[cri]*z)))
            # combined max SNR for all chirps
            idx=n.where(mf[cri,:] > mf_p)[0]
            # find peak match function at each point
            mf_p[idx]=mf[cri,idx]
            # record chirp-rate that produces the highest matched filter output
#            mf_cr[idx]=self.conf.chirp_rates[cri]
            mf_chirp_rate_idx[idx]=cri

            # store snippet of the spectrum
            

        # detect peaks
        snrs=[]
        chirp_rates=[]
        frequencies=[]
        for i in range(self.conf.max_simultaneous_detections):
            mi=n.argmax(mf_p)
            # CLEAN detect peaks
            snr_max=mf_p[mi]
            # this is the center frequency of the dechirped signal
            # corresponds to the instantaneous
            # chirp frequency at the leading edge of the signal
            f0=self.conf.fvec[mi]
            # clear region around detection
            mf_p[n.max([0,mi-self.conf.mfsi]):n.min([mi+self.conf.mfsi,n_samps-1])]=0.0
            # this is the chirp rate we've detected
            detected_chirp_rate=self.conf.chirp_rates[mf_chirp_rate_idx[mi]]

            # did we find a chirp?
            if snr_max > self.conf.threshold_snr:
                # the virtual start time
                chirp_time = t0 - f0/detected_chirp_rate
                debug1("found chirp snr %1.2f chirp-rate %1.2f f0 %1.2f chirp_time %1.4f %s"%(snr_max,detected_chirp_rate/1e3,f0/1e6,chirp_time,unix2datestr(chirp_time)))
                snrs.append(snr_max)
                chirp_rates.append(detected_chirp_rate)
                frequencies.append(f0)

                dname="%s/%s"%(self.conf.output_dir,unix2dirname(float(i0)/self.conf.sample_rate))
                
                if not os.path.exists(dname):
                    print("creating %s"%(dname))
                    os.mkdir(dname)
                    

                # tbd: make an hour directory
                ofname = "%s/chirp-%d.h5"%(dname,
                                           i0)
                ho=h5py.File(ofname,"w")
                ho["f0"]=f0
                ho["i0"]=i0
                ho["sample_rate"]=self.conf.sample_rate
                ho["n_samples"]=n_samps
                ho["chirp_time"]=chirp_time
                ho["chirp_rate"]=detected_chirp_rate
                ho["snr"]=snr_max
                debug1("saving %s"%(ofname))
                ho.close()
            
        cput1=time.time()

        data_dt=(n_samps/float(self.conf.sample_rate))

        return(snrs,chirp_rates,frequencies)
        
