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
import pyfftw
import h5py
import scipy.constants as c

def power(x):
    return(x.real**2.0 + x.imag**2.0)

def fft(x):
    return(pyfftw.interfaces.numpy_fft.fft(x,planner_effort='FFTW_ESTIMATE'))

def ifft(x):
    return(pyfftw.interfaces.numpy_fft.ifft(x,planner_effort='FFTW_ESTIMATE'))    

debug_out0=False
def debug0(msg):
    if debug_out0:
        print(msg)
debug_out1=True
def debug1(msg):
    if debug_out1:
        print(msg)

class chirp_config:
    def __init__(self,
                 n_samples_per_block=2500000,
                 sample_rate=25000000.0,
                 center_freq=12.5e6,
                 chirp_rates=[50e3,100e3,125e3,500.0084e3,550e3],
                 minimum_frequency_spacing=0.2e6,
                 threshold_snr=15.0,
                 max_simultaneous_detections=5,
                 save_bandwidth=20e3, # how much bandwidth do we store around detected peak
                 output_dir="chirp_out"):

        self.n_samples_per_block=n_samples_per_block
        self.sample_rate=sample_rate
        self.center_freq=center_freq
        self.chirp_rates=chirp_rates

        os.system("mkdir -p %s"%(output_dir))
        self.output_dir=output_dir
        # the minimum distance in frequency between detections
        # (avoid multiple detections of the same chirp)
        self.minimum_frequency_spacing=minimum_frequency_spacing
        self.df=(float(sample_rate)/float(n_samples_per_block))
        self.mfsi=int(minimum_frequency_spacing/self.df) # minimum spacing of detections in fft bins

        # how many chirps can we detect simultaneously
        self.max_simultaneous_detections=max_simultaneous_detections
        # the smallest normalized snr that is detected
        self.threshold_snr=threshold_snr

        self.fvec=n.fft.fftshift(n.fft.fftfreq(n_samples_per_block,
                                               d=1.0/float(sample_rate)))+center_freq
        
        self.save_bandwidth=save_bandwidth
        n_bins_to_save=int(save_bandwidth/self.df)
        self.save_freq_idx=n.arange(-int(n_bins_to_save/2),int(n_bins_to_save/2),dtype=n.int)
        self.save_len=len(self.save_freq_idx)

class chirp_matched_filter_bank:
    def __init__(self,conf):
        self.conf=conf

        # create chirp signal vectors
        # centered around zero frequency
        self.chirps=[]
        self.wf=ss.hann(self.conf.n_samples_per_block)
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
                debug1("found chirp snr %1.2f chirp-rate %1.2f f0 %1.2f chirp_time %1.2f"%(snr_max,detected_chirp_rate/1e3,f0/1e6,chirp_time))
                snrs.append(snr_max)
                chirp_rates.append(detected_chirp_rate)
                frequencies.append(f0)

                # we're going to store these frequency bins
                save_idx=(self.conf.save_freq_idx + mi)%n_samps
                # this is the portion of the spectrum that we save
                store_spec = mf[mf_chirp_rate_idx[mi],save_idx]
                ofname = "%s/chirp-%1.2f-%d.h5"%(self.conf.output_dir,
                                                 detected_chirp_rate/1e3,
                                                 i0)
                ho=h5py.File(ofname,"w")
                ho["spec"]=store_spec
                ho["f0"]=f0
                ho["fvec"]=self.conf.fvec[save_idx]
                ho["i0"]=i0
                ho["sample_rate"]=self.conf.sample_rate
                ho["chirp_time"]=chirp_time
                ho["chirp_rate"]=detected_chirp_rate
                debug1("saving %s"%(ofname))
                ho.close()
            
        cput1=time.time()

        data_dt=(n_samps/float(self.conf.sample_rate))
        debug0("speed %1.2f x realtime"%( data_dt/(cput1-cput0) ))
        return(snrs,chirp_rates,frequencies)
        
