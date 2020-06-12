import uhd
import numpy as n
import argparse
import scipy.signal as ss
import matplotlib.pyplot as plt
import os
import time
import threading
import glob

def cleanup_files(n_files=100):
    """
    cleanup files so that we don't run out of ram
    """
    while True:
        fl=glob.glob("/dev/shm/tmp*.bin")
        fl.sort()
        n_on_disk=len(fl)
        print("%d/%d files on disk"%(len(fl),n_files))
        n_to_delete = n_on_disk - n_files
        if n_to_delete > 0:
            print("deleting %d files"%(n_to_delete))
            for i in range(n_to_delete):
                try:
                    print("deleting %s"%(fl[i]))
                    os.system("rm %s"%(fl[i]))
                except:
                    pass
        time.sleep(1)
                
def main():
    os.system("rm /dev/shm/*.bin")
    n_samples=2000000
    sr=20e6
    cf=10e6

    cleanup = threading.Thread(target=cleanup_files)
    cleanup.daemon=True
    cleanup.start()
    
    u = uhd.usrp.MultiUSRP("recv_buff_size=500000000")
    
    # set the start seconds using ntp time
    # wait until the second has changed, then setup the internal clock with the next pps 
    ts=u.get_time_last_pps()
    t0=ts.get_full_secs()+ts.get_frac_secs()

    ts2=u.get_time_last_pps()
    t1=ts2.get_full_secs()+ts2.get_frac_secs()

    while t0 == t1:
        time.sleep(0.1)
        ts2=u.get_time_last_pps()
        t1=ts2.get_full_secs()+ts2.get_frac_secs()
        
    u.set_time_next_pps(uhd.types.TimeSpec(n.floor(time.time())+1.0))

    time.sleep(2)
    ts2=u.get_time_last_pps()
    t1=ts2.get_full_secs()+ts2.get_frac_secs()
    
    u.set_rx_subdev_spec(uhd.usrp.SubdevSpec("A:B"))
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
    st.issue_stream_cmd(st_cmd)

    recv_samps=0
    
    idx=n.arange(buffer_samps,dtype=n.int64)
    fi=0
    n_files=40
    while True:
        # keep reading packets as fast as we can
        samps=st.recv(recv_buffer,md)
        if samps > 0:
            ringbuffer[hi:(hi+samps)]=recv_buffer[0:samps]
            hi=hi+samps
            # hi-samps = si
            si=int(md.time_spec.get_full_secs()*sr)+int(md.time_spec.get_frac_secs()*sr)
        
            if hi > n_samples:
                fname="/dev/shm/tmp-%09d.bin"%(fi)
                z=ringbuffer[0:n_samples]
                fo=open(fname,"w")
                # time of first sample 
                si=n.int64(si-(hi-samps))
                si.tofile(fo)
                z.tofile(fo)
                fo.close()
                fi=fi+1

                leftover=hi-n_samples
                ringbuffer[0:leftover]=ringbuffer[n_samples:(n_samples+leftover)]
                hi=leftover
        else:
            print("dropped a packet. no biggie.")
    

if __name__ == "__main__":
    main()
