#!/usr/bin/env python
#
# Scan through a digital rf recording
#
import numpy as n
import chirp_det as c
import chirp_config as cc
import digital_rf as drf
from mpi4py import MPI

comm=MPI.COMM_WORLD
size=comm.Get_size()
rank=comm.Get_rank()

def scan_for_chirps():

    conf=cc.chirp_config()
    
    cfb=c.chirp_matched_filter_bank(conf)

    d=drf.DigitalRFReader(conf.data_dir)
    b=d.get_bounds(conf.channel)
    idx0=b[0]
    
    n_blocks=(b[1]-idx0)/conf.n_samples_per_block

    # mpi scan through dataset
    for block_idx in range(rank,n_blocks,size):
        i0=block_idx*conf.n_samples_per_block + idx0
        z=d.read_vector_c81d(i0,conf.n_samples_per_block,conf.channel)
        snrs,chirp_rates,f0s=cfb.seek(z,i0)

if __name__ == "__main__":
    scan_for_chirps()
    
