import ctypes
import numpy as n
from ctypes.util import find_library
from numpy import ctypeslib
import matplotlib.pyplot as plt

libdc=ctypes.cdll.LoadLibrary("./libdownconvert.so")
libdc.test.argtypes =[ ctypeslib.ndpointer(n.complex64,ndim=1,flags='C'), ctypes.c_int ]
libdc.consume.argtypes =[ ctypes.c_double,
                          ctypes.c_double,
                          ctypeslib.ndpointer(n.complex64,ndim=1,flags='C'),
                          ctypes.c_int,
                          ctypeslib.ndpointer(n.complex64,ndim=1,flags='C'),
                          ctypeslib.ndpointer(n.complex64,ndim=1,flags='C'),
                          ctypes.c_int,
                          ctypes.c_int,
                          ctypes.c_int,
                          ctypes.c_double,
                          ctypes.c_double ]
                          
class chirp_downconvert:
    def __init__(self,
                 tab_len=8192,
                 f0=-12.5e6,
                 rate=100e3,
                 dec=2500,
                 dec2=5000,
                 dt=1.0/25e6):
        
        self.chirpt=0.0
        # conjugate sinusoid!
        self.sintab=n.array(n.exp(-1j*2.0*n.pi*n.arange(tab_len)/tab_len),dtype=n.complex64)
        self.tab_len=tab_len
        self.f0=f0
        self.rate=rate
        self.dec=dec
        self.dec2=dec2
        self.dt=dt
        
    def consume(self,
                 z_in,
                 z_out,
                 n_out):
        #void consume(double chirpt, double dt, complex_float *sintab, int tabl, complex_float *in, complex_float *out_buffer, int n_in, int dec, int dec2, double f0, double rate)
        if (len(z_in)-self.dec2)/self.dec < n_out:
            print("not enough input samples")
        libdc.consume(self.chirpt,
                      self.dt,
                      self.sintab,
                      self.tab_len,
                      z_in,
                      z_out,
                      n_out,
                      self.dec,
                      self.dec2,
                      self.f0,
                      self.rate)
        self.chirpt+=float(n_out*self.dec)*self.dt


def chirp(L,f0=-12.5e6,cr=100e3,sr=25e6):
    """
    Generate a chirp.
    """
    tv=n.arange(L,dtype=n.float64)/sr
    dphase=0.5*tv**2*cr*2*n.pi
    chirpv=n.array(n.exp(1j*n.mod(dphase,2*n.pi))*n.exp(1j*2.0*n.pi*f0*tv),dtype=n.complex64)
    return(chirpv)
        
if __name__ == "__main__":
    cdc=chirp_downconvert()
    
    z_in=chirp(L=25000000+5000)
    z_out=n.zeros(1000,dtype=n.complex64)
    cdc.consume(z_in,z_out,1000)
    import matplotlib.pyplot as plt
    plt.plot(z_out.real)
    plt.plot(z_out.imag)
    plt.show()
                    
