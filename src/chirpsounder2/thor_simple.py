import uhd
import numpy as n
import argparse
import scipy.signal as ss
import matplotlib.pyplot as plt
import os
import time
import digital_rf as drf
import threading

def write_to_file(drf_out,recv_buffer,n_packets):
    pass
#    for i in range(n_packets):
 #       drf_out.rf_write(recv_buffer[0,:])


def main():
    
    cpu_dtype = n.dtype([(str("r"), n.int16), (str("i"), n.int16)])
  
    
    n_samples=363
    sr=25e6
    sample_rate_i=int(25000000)
    cf=12.5e6
    
    u = uhd.usrp.MultiUSRP("addr=192.168.10.2,recv_buff_size=500000000")

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
    t0=n.floor(time.time())+1.0
    u.set_time_next_pps(uhd.types.TimeSpec(t0))

    start_sample=int(t0)*sample_rate_i

    drf_out = drf.DigitalRFWriter("/dev/shm/hf1/cha",
                                  cpu_dtype,
                                  3600,
                                  1000,
                                  start_sample,
                                  sample_rate_i,
                                  1.0,
                                  "qweewqewq",
                                  compression_level=0,
                                  checksum=False,
                                  is_complex=True,
                                  is_continuous=True,
                                  marching_periods=True)

    
    time.sleep(2)
    ts2=u.get_time_last_pps()
    t1=ts2.get_full_secs()+ts2.get_frac_secs()
    print(t1)
    
    u.set_rx_subdev_spec(uhd.usrp.SubdevSpec("A:A"))
    u.set_rx_rate(sr)
    u.set_rx_freq(uhd.types.TuneRequest(cf))


    samples=n.empty((1,n_samples),dtype=cpu_dtype)
    st_args=uhd.usrp.StreamArgs("sc16","sc16")
    st_args.channels=[0]

    md=uhd.types.RXMetadata()
    st=u.get_rx_stream(st_args)
    
    buffer_samps=st.get_max_num_samps()
    recv_buffer=n.zeros((1,buffer_samps),dtype=cpu_dtype)

    st_cmd=uhd.types.StreamCMD(uhd.types.StreamMode.start_cont)
    st_cmd.stream_now=True
    st.issue_stream_cmd(st_cmd)
    # tbd, set gps time

    si_prev=int(0)


    wr_thread=None
    ssum=0.0
    while True:
        samps=st.recv(recv_buffer,md)

        si=int(md.time_spec.get_full_secs())*sample_rate_i+int(md.time_spec.get_frac_secs()*sr)
        if samps != 363:
            print("got %d samps"%(samps))

        if si-si_prev != 363 and si_prev != 0:
            n_samps=si-si_prev
            n_packets=int(n_samps/363)
            print("skipping %d packets %d"%(n_packets,n_samps))
        si_prev=si

if __name__ == "__main__":
    main()
