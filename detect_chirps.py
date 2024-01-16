#!/usr/bin/env python
#
# Scan through a digital rf recording
#
import numpy as n
import chirp_det as c
import chirp_config as cc
import digital_rf as drf
from mpi4py import MPI
import time
import sys
import traceback
import os.path

comm=MPI.COMM_WORLD
size=comm.Get_size()
rank=comm.Get_rank()

def kill(conf):
    exists = os.path.isfile(conf.kill_path)
    return exists

def scan_for_chirps(conf,cfb,block0=[None]):
    d=drf.DigitalRFReader(conf.data_dir)
    channelnames = d.get_channels()
    b0=d.get_bounds(channelnames[0])
    b1=d.get_bounds(channelnames[1])
   # print(b0)
   # print(b1)
    if block0 == [None]:
        block0 = [int(n.ceil(b0[0]/(conf.n_samples_per_block*conf.step))),int(n.ceil(b1[0]/(conf.n_samples_per_block*conf.step)))]

   # print(block0)
    block1 = [int(n.floor(b0[1]/(conf.n_samples_per_block*conf.step))),int(n.floor(b1[1]/(conf.n_samples_per_block*conf.step)))]

    # mpi scan through dataset
    for ch in range(len(channelnames)):

        for block_idx in range(block0[ch],block1[ch]):
            print('block_idx: %i' % block_idx)
            if block_idx%size == rank:
                # this is my block!
                try:
                    cput0=time.time()
                    # we may skip over data (step > 1) to speed up detection
                    i0=block_idx*conf.n_samples_per_block*conf.step
                    #            i0=block_idx*conf.n_samples_per_block*conf.step + idx0
                    # read vector from recording
                    z=d.read_vector_c81d(i0,conf.n_samples_per_block,channelnames[ch])
                    snrs,chirp_rates,f0s=cfb.seek(z,i0,channelnames[ch])
                    cput1=time.time()
                    analysis_time=(conf.n_samples_per_block*conf.step)/conf.sample_rate
                    print("%d/%d Analyzing %s speed %1.2f * realtime"%( rank,
                                                                        size,
                                                                        c.unix2datestr(i0/conf.sample_rate),
                                                                        size*analysis_time/(cput1-cput0) ))
                except:
                    print("error")
                    traceback.print_exc()
    return(block1)

if __name__ == "__main__":
    if len(sys.argv) == 2:
        conf=cc.chirp_config(sys.argv[1])
    else:
        conf=cc.chirp_config()
        
    cfb=c.chirp_matched_filter_bank(conf)
        
    if not conf.realtime:
        scan_for_chirps(conf,cfb)
    else:
        block1=[None]
        first=1
        while True:
            if kill(conf):
                print("kill.txt found, stopping detect_chirps.py")
                sys.exit(0)
            else:
                if first==1:
                    time.sleep(11)
                    block1=scan_for_chirps(conf,cfb,block1)
                    time.sleep(0.001)
                    first = 0
                else:
                    block1=scan_for_chirps(conf,cfb,block1)
                    time.sleep(0.001)
