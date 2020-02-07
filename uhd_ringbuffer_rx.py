import uhd
import numpy as n
import argparse
import scipy.signal as ss
import matplotlib.pyplot as plt
import os
import time

def main():
    os.system("rm /dev/shm/*.bin")
    n_samples=2000000
    sr=20e6
    cf=10e6
    
    u = uhd.usrp.MultiUSRP("addr=192.168.10.3")

    
    # set the start seconds using ntp time
    # wait until the second has changed, then setup the internal clock with the next pps 
    ts=u.get_time_last_pps()
    print(ts.get_full_secs()+ts.get_frac_secs())
    t0=ts.get_full_secs()+ts.get_frac_secs()

    ts2=u.get_time_last_pps()
    t1=ts2.get_full_secs()+ts2.get_frac_secs()
    
    while t0 == t1:
        print("waiting")
        time.sleep(0.1)
        ts2=u.get_time_last_pps()
        t1=ts2.get_full_secs()+ts2.get_frac_secs()
        
    print("done")
    u.set_time_next_pps(uhd.types.TimeSpec(n.floor(time.time())+1.0))

    time.sleep(2)
    ts2=u.get_time_last_pps()
    t1=ts2.get_full_secs()+ts2.get_frac_secs()
    print(t1)
    
    
    u.set_rx_subdev_spec(uhd.usrp.SubdevSpec("A:A"))

    u.set_rx_rate(sr)
    u.set_rx_freq(uhd.types.TuneRequest(cf))

    samples=n.empty((1,n_samples),dtype=n.complex64)
    st_args=uhd.usrp.StreamArgs("fc32","sc16")
    st_args.channels=[0]



    
    md=uhd.types.RXMetadata()
    st=u.get_rx_stream(st_args)
    
    buffer_samps=st.get_max_num_samps()
    recv_buffer=n.zeros((1,buffer_samps),dtype=n.complex64)


    ringbuffer=n.zeros(2*n_samples,dtype=n.complex64)
    hi=0
    L=len(ringbuffer)

    st_cmd=uhd.types.StreamCMD(uhd.types.StreamMode.start_cont)
    st_cmd.stream_now=True
    print(st_cmd)
    st.issue_stream_cmd(st_cmd)
    # tbd, set gps time

    recv_samps=0
    
    idx=n.arange(buffer_samps,dtype=n.int64)
    fi=0
    n_files=40
    while True:
        samps=st.recv(recv_buffer,md)
        if samps > 0:
            ringbuffer[hi:(hi+samps)]=recv_buffer[0:samps]
            hi=hi+samps
            # hi-samps = si
            si=int(md.time_spec.get_full_secs()*sr)+int(md.time_spec.get_frac_secs()*sr)
        
            if hi > n_samples:
                #            si=int(md.time_spec.get_full_secs()*sr)+int(md.time_spec.get_frac_secs()*sr)
                fname="/dev/shm/tmp-%09d.bin"%(fi)
                z=ringbuffer[0:n_samples]
                fo=open(fname,"w")
                # time of first sample 
                si=n.int64(si-(hi-samps))
                si.tofile(fo)
                z.tofile(fo)
                fo.close()
                fi=fi+1
                dfname="/dev/shm/tmp-%09d.bin"%(fi-n_files)
                if os.path.exists(dfname):
                    os.system("rm %s"%(dfname))

                leftover=hi-n_samples
                ringbuffer[0:leftover]=ringbuffer[n_samples:(n_samples+leftover)]
                hi=leftover
        else:
            print("No samples received")
                    

    

if __name__ == "__main__":
    main()
