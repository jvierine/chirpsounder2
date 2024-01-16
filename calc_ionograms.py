#!/usr/bin/env python
#
# Scan through a digital rf recording
#
import numpy as np
import numpy as n
import digital_rf as drf
from mpi4py import MPI
import glob
import fast_exp as fe
import scipy.signal as ss
import scipy.constants as c
import h5py
import chirp_config as cc
import chirp_det as cd
import pyfftw
import matplotlib.pyplot as plt
import time
import os
import os.path
import sys
import traceback
import pdb

# c library
import chirp_lib as cl

comm=MPI.COMM_WORLD
size=comm.Get_size()
rank=comm.Get_rank()

def kill(conf):
    exists = os.path.isfile(conf.kill_path)
    return exists


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
    # Changed from m/Hz round trip on 2022.04.10 to
    # m/Hz one-way travel time 
    return(dt*c.c)

def chirp(L,f0=-25e3,cr=160e3,sr=50e3,use_numpy=False):
    """
    Generate a chirp.
    """
    tv=np.arange(L,dtype=np.float64)/sr
    dphase=0.5*tv**2*cr*2*np.pi

    if use_numpy:
        chirp=np.exp(1j*np.mod(dphase,2*np.pi))*np.exp(1j*2*np.pi*f0*tv)
    else:
        # table lookup based faster version
        chirp=fe.expf(dphase)*fe.expf((2*np.pi*f0)*tv)
        #   chirp=fe.expf(dphase+(2*np.pi*f0)*tv)#*fe.expf()
    return(chirp)

def spectrogram(x,window=1024,step=512,wf=ss.hann(1024),n_oversample=13):

    n_spec=int((len(x)-window)/step)

    # n_oversample has to be odd
    if n_oversample%2 == 0:
        n_oversample+=1
        
    n_substep=int(step/n_oversample)
    substep_idx=n.arange(n_oversample,dtype=n.int)- int((n_oversample-1)/2)
    
    S=np.zeros([n_spec,window],dtype=n.float64)
    for i in range(n_spec):
        n_avg=0.0
        for j in range(n_oversample):
            i0=i*step + substep_idx[j]*n_substep
            i1=i*step + substep_idx[j]*n_substep + window
#            print("%d %d"%(i0,step))
            
            if (i0 >= 0) and (i1 < len(x)):
                freq_pwr=np.abs(np.fft.fftshift(np.fft.fft(wf*x[i0:i1])))**2.0
                std_est = n.nanmedian( n.abs(freq_pwr-n.nanmedian(freq_pwr)) )
#                print(std_est)
                if std_est > 0:
                    S[i,:] += (1.0/(std_est**2.0))*freq_pwr#*np.abs(np.fft.fftshift(np.fft.fft(wf*x[i0:i1])))**2.0
                    n_avg+=1.0/(std_est**2.0)
                
        S[i,:]=S[i,:]/n_avg
        
    #normalize scale to float16
    S=5e4*S/n.nanmax(S)
    return(S)

def decimate(x,dec):
    Nout = int(np.floor(len(x)/dec))
    idx = np.arange(Nout,dtype=np.int)*int(dec)
    res = np.zeros(len(idx),dtype=x.dtype)

    for i in np.arange(dec):
        res += x[idx+i]
    return(res/float(dec))

def chirp_downconvert(conf,
                      t0,
                      d,
                      i0,                  
                      ch,
                      rate,
                      dec=2500,
                      realtime_req=None,
                      cid=0):
    cput0=time.time()
    sleep_time=0.0
    sr=conf.sample_rate
    cf=conf.center_freq
    dur=conf.maximum_analysis_frequency/rate
    if realtime_req==None:
        realtime_req=dur
    idx=0
    step=1000
    n_windows=int(dur*sr/(step*dec))+1

    cdc=cl.chirp_downconvert(f0=-cf,
                             rate=rate,
                             dec=dec,
                             dt=1.0/conf.sample_rate,
                             n_threads=conf.n_downconversion_threads)
    
    zd_len=n_windows*step
    zd=np.zeros(zd_len,dtype=np.complex64)
    
    z_out=np.zeros(step,dtype=np.complex64)
    n_out=step
    
    for fi in range(n_windows):
        missing=False
        try:
            if conf.realtime:
                b=d.get_bounds(ch)
                while ((i0+idx+step*dec+cdc.filter_len*dec)+int(conf.sample_rate)) > b[1]:
                    # wait for more data to be acquired
                    # as the tail of the buffer doesn't have he data we
                    # need yet
                    time.sleep(1.0)
                    sleep_time+=1.0
                    b=d.get_bounds(ch)
                    
            z=d.read_vector_c81d(i0+idx,step*dec+cdc.filter_len*dec,ch)
        except:
#            z=np.zeros(step*dec+cdc.filter_len*dec,dtype=np.complex64)
            missing=True
            
        # we can skip this heavy step if there is missing data
        if not missing:
            cdc.consume(z,z_out,n_out)
        else:
            # step chirp time forward
            cdc.advance_time(dec*step)
            z_out[:]=0.0
            
        zd[(fi*step):(fi*step+step)]=z_out
        
        idx+=dec*step

    dr=conf.range_resolution
    df=conf.frequency_resolution
    sr_dec = sr/dec
    ds=get_m_per_Hz(rate)
    fftlen = int(sr_dec*ds/dr/2.0)*2
    fft_step=int((df/rate)*sr_dec)

    S=spectrogram(np.conj(zd),window=fftlen,step=fft_step,wf=ss.hann(fftlen))

    freqs=rate*np.arange(S.shape[0])*fft_step/sr_dec
    range_gates=ds*np.fft.fftshift(np.fft.fftfreq(fftlen,d=1.0/sr_dec))

    if conf.manual_range_extent:
        ridx=np.where( (range_gates > conf.min_range) & (range_gates < conf.max_range) )[0]
    else:
        ridx=np.where( n.abs(range_gates) < conf.max_range_extent)[0]

    fidx=n.arange(len(freqs),dtype=n.int)
    if conf.manual_freq_extent:
        fidx=n.where( (freqs>conf.min_freq) & (freqs < conf.max_freq) )[0]
        

        
    try:
        dname="%s/%s"%(conf.output_dir,cd.unix2dirname(t0))
        if not os.path.exists(dname):
            os.mkdir(dname)
        ofname="%s/lfm_ionogram-%s-%s-%03d-%1.2f.h5"%(dname,conf.station_name,ch,cid,t0)
        print("Writing to %s" % ofname)
        ho=h5py.File(ofname,"w")
        S0=n.array(S[:,ridx],dtype=n.float16)     # ionogram frequency-range, save space
        ho["S"]=S0[fidx,:]
        ho["freqs"]=freqs[fidx]  # frequency bins
        ho["rate"]=rate    # chirp-rate
        ho["ranges"]=range_gates[ridx]
        ho["t0"]=t0
        ho["id"]=cid
        ho["station_name"]=conf.station_name
        ho["sr"]=float(sr_dec) # ionogram sample-rate
        if conf.save_raw_voltage:
            ho["z"]=zd
        ho["ch"]=ch            # channel name
        ho.close()
    except:
        traceback.print_exc(file=sys.stdout)
        print("error writing file")

    cput1=time.time()
    cpu_time=cput1-cput0-sleep_time
    print("Done processed %1.2f s in %1.2f s, speed %1.2f * realtime"%(realtime_req,cpu_time,realtime_req/cpu_time))
    sys.stdout.flush()
    

def analyze_all(conf,d,ch):
    fl=glob.glob("%s/*/par-*.h5"%(conf.output_dir))
    n_ionograms=len(fl)
    # mpi scan through the whole dataset
    for ionogram_idx in range(rank,n_ionograms,size):
        h=h5py.File(fl[ionogram_idx],"r")
        chirp_rate=np.copy(h[("chirp_rate")])
        t0=np.copy(h[("t0")]) 
        i0=np.int64(t0*conf.sample_rate)
        print("calculating i0=%d chirp_rate=%1.2f kHz/s t0=%1.6f"%(i0,chirp_rate/1e3,t0))
        h.close()

        chirp_downconvert(conf,
                          t0,
                          d,
                          i0,                  
                          ch,
                          chirp_rate,
                          dec=2500)

def analyze_realtime(conf,d,ch):
    """ 
    Realtime analysis using analytic timing
    We allocate one MPI process for each sounder to be on the safe side.

    TODO: load chirp timing information dynamically
          and use a process pool to calculate as many chirp ionograms 
          as there are computational resources.
    """
    st=conf.sounder_timings[rank]
    n_sounders=len(st)    

    while True:    
        if kill(conf):
            print("kill.txt found, stopping calc_ionograms.py")
            sys.exit(0)
        else:
            b=d.get_bounds(ch)
            t0=np.floor(np.float128(b[0]) / np.float128(conf.sample_rate))
            t1=np.floor(np.float128(b[1]) / np.float128(conf.sample_rate))

            # find the next sounder that can be measured with shortest wait time
            best_sounder=0
            best_wait_time=1e6
            best_t0=0
            best_id=0
            for s_idx in range(n_sounders):
                rep=np.float128(st[s_idx]["rep"])
                chirpt=np.float128(st[s_idx]["chirpt"])
                chirp_rate=st[s_idx]["chirp-rate"]
                cid=st[s_idx]["id"]
                
                try_t0=rep*np.floor(t0/rep)+chirpt
                while try_t0 < t0:
                    try_t0+=rep
                wait_time = try_t0-t0

                if wait_time < best_wait_time:
                    best_sounder=s_idx
                    best_t0=try_t0
                    best_wait_time=wait_time
                    best_id=cid
            rep=np.float128(st[best_sounder]["rep"])
            chirpt=np.float128(st[best_sounder]["chirpt"])
            chirp_rate=st[best_sounder]["chirp-rate"]
            next_t0=float(best_t0)
            print("Rank %d chirp id %d analyzing chirp-rate %1.2f kHz/s chirpt %1.4f rep %1.2f"%(rank,best_id,chirp_rate/1e3,chirpt,rep))
            i0=int(next_t0*conf.sample_rate)
            realtime_req=conf.sample_rate/chirp_rate
            print("Buffer extent %1.2f-%1.2f launching next chirp at %1.2f %s"%(b[0]/conf.sample_rate,
                                                                                b[1]/conf.sample_rate,
                                                                                next_t0,
                                                                                cd.unix2datestr(next_t0)))


            chirp_downconvert(conf,
                              next_t0,
                              d,
                              i0,                  
                              ch,
                              chirp_rate,
                              realtime_req=realtime_req,
                              dec=conf.decimation,
                              cid=best_id)


def get_next_chirp_par_file(conf, d,ch):
    """ 
    wait until we encounter a parameter file with remaining time 
    """
    # find the next sounder that can be measured
    while True:
        if kill(conf):
            print("kill.txt found, stopping calc_ionograms.py")
            sys.exit(0)
        else:
            b=d.get_bounds(ch)
            buffer_t0=np.floor(np.float128(b[0])/np.float128(conf.sample_rate))
            while np.isnan(buffer_t0):
                b=d.get_bounds(ch)
                buffer_t0=np.floor(np.float128(b[0])/np.float128(conf.sample_rate))
                # t1=np.floor(np.float128(b[1])/np.float128(conf.sample_rate))
                print("nan bounds for ringbuffer. trying again")
                time.sleep(1)

            
            # todo: look at today and yesterday. only looking
            # at today will result in a few lost ionograms
            # when the day is changing
            dname="%s/%s"%(conf.output_dir,cd.unix2dirname(time.time()))
            fl=glob.glob("%s/par-%s*.h5"%(dname,ch))
            fl.sort()
            
            if len(fl)> 0:
                for fi in range(len(fl)):
                    ftry = fl[len(fl)-fi-1]

                    # proceed if this hasn't already been analyzed.
                    if not os.path.exists("%s.done"%(ftry)):
                        h=h5py.File(ftry,"r")
                        t0=float(np.copy(h[("t0")]))
                        i0=np.int64(t0*conf.sample_rate)
                        chirp_rate=float(np.copy(h[("chirp_rate")]))
                        h.close()
                        t1=conf.maximum_analysis_frequency/chirp_rate + t0
                        
                        tnow=time.time()

                        # if the beginning of the buffer is before the end of the chirp,
                        # start analyzing as there is at least some of the the ionogram
                        # still in the buffer. the start of the buffer is
                        # before the the chirp ends
                        # t0 ---- t1
                        #      bt0-------bt1
                        if buffer_t0 < t1:
                            # if not already analyzed, analyze it
                            if not os.path.exists("%s.done"%(ftry)):
                                ho=h5py.File("%s.done"%(ftry),"w")
                                ho["t_an"]=time.time()
                                ho.close()
                                print("Rank %d analyzing %s time left in sweep %1.2f s"%(rank,ftry,t1-tnow))
                                return(ftry)
                        else:
                            # we haven't analyzed this one, but we no longer
                            # can, because it is not in the buffer
                            print("Not able to analyze %s (%1.2f kHz/s), because it is no longer in the buffer. Buffer start at %1.2f and chirp ends at %1.2f"%(ftry,chirp_rate/1e3,buffer_t0,t1))
                            ho=h5py.File("%s.done"%(ftry),"w")
                            ho["t_an"]=time.time()
                            ho.close()
                            time.sleep(0.01)
                                
            # didn't find anything. let's wait.
            time.sleep(1)

        
def analyze_parfiles(conf,d,ch):
    """ 
    Realtime analysis using newly found parameter files.
    """
    while True:
        if kill(conf):
            print("kill.txt found, stopping calc_ionograms.py")
            sys.exit(0)
        else:
            ftry=get_next_chirp_par_file(conf,d,ch)
            
            h=h5py.File(ftry,"r")
            t0=float(np.copy(h[("t0")]))
            i0=np.int64(t0*conf.sample_rate)
            chirp_rate=float(np.copy(h[("chirp_rate")]))
            h.close()
            
            chirp_downconvert(conf,
                              t0,
                              d,
                              i0,                  
                              ch,
                              chirp_rate,
                              dec=conf.decimation,
                              cid=0)
            
            time.sleep(0.1)
            

if __name__ == "__main__":
    if len(sys.argv) == 3:
        conf=cc.chirp_config(sys.argv[1])
    else:
        conf=cc.chirp_config()
    

    
    # analyze serendpituous par files immediately after a chirp is detected
    if conf.serendipitous:
        # avoid having two processes snag the same sounder at the start
        time.sleep(rank)
        first=1
        while True:
            if kill(conf):
                print("kill.txt found, stopping calc_ionograms.py")
                sys.exit(0)
            else:
                if first==1:
                    try:
                        time.sleep(10)
                        d=drf.DigitalRFReader(conf.data_dir)
                        analyze_parfiles(conf,d,sys.argv[2])
                        first = 0
                    except:
                        print("error in calc_ionograms.py. trying to restart")
                        traceback.print_exc(file=sys.stdout)
                        sys.stdout.flush()
                        time.sleep(1)

                else:
                    try:
                        d=drf.DigitalRFReader(conf.data_dir)
                        analyze_parfiles(conf,d,sys.argv[2])
                    except:
                        print("error in calc_ionograms.py. trying to restart")
                        traceback.print_exc(file=sys.stdout)
                        sys.stdout.flush()
                        time.sleep(1)
                
        
        
    elif conf.realtime: # analyze analytic timings
        while True:
            if kill(conf):
                print("kill.txt found, stopping calc_ionograms.py")
                sys.exit(0)
            else:
                try:
                    d=drf.DigitalRFReader(conf.data_dir)
                    analyze_realtime(conf,d,sys.argv[2])
                except:
                    print("error in calc_ionograms.py. trying to restart")
                    traceback.print_exc(file=sys.stdout)
                    sys.stdout.flush()
                    time.sleep(1)
        
    else: # batch analyze
        d=drf.DigitalRFReader(conf.data_dir)
        analyze_all(conf,d,sys.argv[2])




    
