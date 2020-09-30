#!/usr/bin/env python

import numpy as n
import matplotlib.pyplot as plt
import scipy.interpolate as si
import time

table_len=1024
phase=2.0*n.pi*n.arange(table_len)/table_len
table=n.array(n.exp(1j*phase),dtype=n.complex64)

def fast_exp(t,freq=10.0):
    """
    complex sinusoid
    """ 
    table_idx = n.array(table_len*n.fmod(2.0*n.pi*t*freq,2.0*n.pi)/2.0/n.pi,dtype=n.int)
    return(table[table_idx])


def expf(phase):
    """
    exp(1j*phase)
    """
    twopi=2.0*n.pi
    table_idx = n.array(table_len*n.fmod(phase,twopi)/twopi,dtype=n.int)
    return(table[table_idx])

if __name__ == "__main__":
    
    t=n.arange(1000000)/1e6
    t0=time.time()
    for i in range(10):
        z0=n.exp(1j*t*10.0*2*n.pi)
    t1=time.time()
    plt.plot(z0.real)
    plt.show()    
    print(t1-t0)
    t0=time.time()    
    for i in range(10):
        z1=fast_exp(t,freq=10.0)
    t1=time.time()
    plt.plot(z1.real)
    plt.show()    
    
    print(t1-t0)
