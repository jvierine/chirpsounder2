
import numpy as n
import argparse
import scipy.signal as ss
import matplotlib.pyplot as plt
import time
import glob
import re
import os
from mpi4py import MPI
import scipy.fftpack 
import pyfftw
import h5py

def power(x):
    return(x.real**2.0 + x.imag**2.0)

def fft(x):
    return(pyfftw.interfaces.numpy_fft.fft(x,planner_effort='FFTW_ESTIMATE'))

def ifft(x):
    return(pyfftw.interfaces.numpy_fft.ifft(x,planner_effort='FFTW_ESTIMATE'))    

comm=MPI.COMM_WORLD
size=comm.Get_size()
rank=comm.Get_rank()
print(comm.Get_size())
print(comm.Get_rank())

def chirpf(L,f0=-25e3,cr=160e3,sr=50e3):
    """
    Generate a chirp. This is used for matched filtering
    """
    tv=n.arange(L,dtype=n.float64)/sr
    dphase=0.5*tv**2*cr*2*n.pi
    chirp=n.exp(1j*n.mod(dphase,2*n.pi))*n.exp(1j*2*n.pi*f0*tv)
    return(chirp)

def main():
    n_samples=2000000
    sr=20e6
    cf=10e6
    fvec=n.fft.fftshift(n.fft.fftfreq(n_samples,d=1/sr))+cf
    # search for these chirp-rates (Hz/s)
    crs=[50e3,100e3,125e3,500.0084e3,550e3]
    chirps=[]
    wf=ss.hann(n_samples)
    done=-1
    for cr in crs:
        chirps.append(n.array(wf*n.conj(chirpf(n_samples,f0=0.0,cr=cr,sr=sr)),dtype=n.complex64))

    # open file to store results
    fo=open("mf_p_%d_%d.bin"%(rank,int(time.time())),"w")

    while True:
        cput0=time.time()
        nums=[]
        fl=glob.glob("/dev/shm/*.bin")
        fl.sort()
        # wait until we have enough data
        while len(fl)< 10:
            fl=glob.glob("/dev/shm/*.bin")
            fl.sort()
            time.sleep(1)         

        # divide files to different threads
        fname=""
        for i in range(size,len(fl)):
            num=int(re.search(".*-(.*).bin",fl[i]).group(1))
            if num%size == rank and num > done:
                fname=fl[i]
                done=num
                break
            
        print("rank %d opening file %s"%(rank,fname))
        try:
            f=open(fname,"r")
            sii=n.fromfile(f,count=1,dtype=n.int64)
            z=n.fromfile(f,count=n_samples,dtype=n.complex64)
            f.close()

            print("rank %d deleting %s"%(rank,fname))
            cmd="rm %s"%(str(fname))
            os.system(cmd)

            # whiten noise
            Z=fft(wf*z)
            z=ifft(Z/(n.abs(Z)+1e-9))

            # matched filter output
            mf_p = n.zeros(n_samples,dtype=n.float32)
            mf_cr = n.zeros(n_samples,dtype=n.float32)
        
            for cri in range(len(crs)):
                mf=power(n.fft.fftshift(fft(wf*chirps[cri]*z)))
                idx=n.where(mf > mf_p)[0]
                # find peak match function at each point
                mf_p[idx]=mf[idx]
                # record chirp-rate that produces the highest matched filter output
                mf_cr[idx]=crs[cri]
        except:
            print("problem reading file %s"%(fname))

        det_p=[]
        det_cr=[]
        det_f0=[]
        # detect peaks
        for i in range(5):
            mi=n.argmax(mf_p)
            # CLEAN detect peaks
            p_max=mf_p[mi]
            f0=fvec[mi]
            mf_p[n.max([0,mi-100000]):n.min([mi+100000,len(mf_p)-1])]=0.0
            
            if p_max > 12.0:
                det_p.append(p_max)
                det_cr.append(mf_cr[mi])
                det_f0.append(f0)
                
                pf=n.float64(p_max)
                cf=n.float64(mf_cr[mi])
                ff=n.float64(f0)
                if0=n.int64(sii)
                header=n.float64(0.0)
                header.tofile(fo)
                pf.tofile(fo)   # matched filter output value
                cf.tofile(fo)   # chirp-rate
                ff.tofile(fo)   # center frequency
                if0.tofile(fo)  # time of detection (samples since 1970)
                fo.flush()
                
        cput1=time.time()
        cpudt=(cput1-cput0)*1e3
        print("Time %1.2f (ms)"%(cpudt))

if __name__ == "__main__":
    main()
