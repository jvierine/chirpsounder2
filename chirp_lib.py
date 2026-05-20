import ctypes
import numpy as n
from ctypes.util import find_library
from numpy import ctypeslib
import matplotlib.pyplot as plt
import scipy.signal as ss
libdc = ctypes.cdll.LoadLibrary("./libdownconvert.so")
libdc.test.argtypes = [ctypeslib.ndpointer(
    n.complex64, ndim=1, flags='C'), ctypes.c_int]
libdc.downconvert_compiled_with_avx.restype = ctypes.c_int
libdc.downconvert_compiled_with_sse.restype = ctypes.c_int
libdc.consume.argtypes = [ctypes.c_double,
                          ctypes.c_double,
                          ctypeslib.ndpointer(n.complex64, ndim=1, flags='C'),
                          ctypes.c_int,
                          ctypeslib.ndpointer(n.complex64, ndim=1, flags='C'),
                          ctypeslib.ndpointer(n.complex64, ndim=1, flags='C'),
                          ctypes.c_int,
                          ctypes.c_int,
                          ctypes.c_int,
                          ctypes.c_double,
                          ctypes.c_double,
                          ctypeslib.ndpointer(n.float32, ndim=1, flags='C'),
                          ctypes.c_int]
libdc.consume_cic.argtypes = [ctypes.c_double,
                              ctypes.c_double,
                              ctypeslib.ndpointer(n.complex64, ndim=1, flags='C'),
                              ctypes.c_int,
                              ctypeslib.ndpointer(n.complex64, ndim=1, flags='C'),
                              ctypeslib.ndpointer(n.complex64, ndim=1, flags='C'),
                              ctypes.c_int,
                              ctypes.c_int,
                              ctypes.c_double,
                              ctypes.c_double,
                              ctypeslib.ndpointer(n.complex64, ndim=1, flags='C'),
                              ctypeslib.ndpointer(n.complex64, ndim=1, flags='C'),
                              ctypes.c_int]

print("libdownconvert SIMD: AVX=%s SSE=%s" %
      (bool(libdc.downconvert_compiled_with_avx()),
       bool(libdc.downconvert_compiled_with_sse())))


class chirp_downconvert:
    def __init__(self,
                 tab_len=8192,
                 f0=-12.5e6,
                 rate=100e3,
                 dec=2500,
                 filter_len=2,
                 n_threads=4,
                 dt=1.0 / 25e6,
                 fast_boxcar_filter=False,
                 downconversion_filter="fir",
                 cic_stages=2):

        # let's add a windowed low pass filter to make this nearly perfect.

        # normalized cutoff freq
        self.n_threads = n_threads
        # om0
        self.om0 = 2.0 * n.pi / float(dec)
        if fast_boxcar_filter and downconversion_filter == "fir":
            downconversion_filter = "boxcar"
        if downconversion_filter not in ["fir", "boxcar", "cic"]:
            raise ValueError("downconversion_filter must be 'fir', 'boxcar', or 'cic'")
        self.downconversion_filter = downconversion_filter
        self.cic_stages = cic_stages
        self.cic_integrator_state = n.zeros(cic_stages, dtype=n.complex64)
        self.cic_comb_state = n.zeros(cic_stages, dtype=n.complex64)

        if downconversion_filter == "cic":
            filter_len = 1
            self.dec2 = dec
            self.m = n.arange(dec, dtype=n.float32)
            self.wfun = n.ones(dec, dtype=n.float32) / float(dec)
        elif downconversion_filter == "boxcar":
            filter_len = 1
            self.dec2 = dec
            self.m = n.arange(dec, dtype=n.float32)
            self.wfun = n.ones(dec, dtype=n.float32) / float(dec)
        else:
            self.dec2 = filter_len * dec
            self.m = n.array(n.arange(filter_len * dec) - dec, dtype=n.float32)
            # windowed low pass filter
            self.wfun = n.array(ss.windows.hann(len(self.m)) * n.sin(self.om0 *
                                (self.m + 1e-6)) / (n.pi * (self.m + 1e-6)), dtype=n.float32)
        # the window function could be twice the decimation rate
        self.chirpt = 0.0
        # conjugate sinusoid!
        self.sintab = n.array(
            n.exp(-1j * 2.0 * n.pi * n.arange(tab_len) / tab_len), dtype=n.complex64)
        self.tab_len = tab_len
        self.f0 = f0
        self.rate = rate
        self.dec = dec
        self.dt = dt
        self.filter_len = filter_len

    def consume(self,
                z_in,
                z_out,
                n_out):
        # void consume(double chirpt, double dt, complex_float *sintab, int tabl, complex_float *in, complex_float *out_buffer, int n_in, int dec, int dec2, double f0, double rate)
        if (len(z_in) - self.dec2) / self.dec < n_out:
            print("not enough input samples %d %d %d %d" %
                  (len(z_in), self.dec2, self.dec, n_out))
        if self.downconversion_filter == "cic":
            libdc.consume_cic(self.chirpt,
                              self.dt,
                              self.sintab,
                              self.tab_len,
                              z_in,
                              z_out,
                              n_out,
                              self.dec,
                              self.f0,
                              self.rate,
                              self.cic_integrator_state,
                              self.cic_comb_state,
                              self.cic_stages)
        else:
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
                          self.rate,
                          self.wfun,
                          self.n_threads)

        self.chirpt += float(n_out * self.dec) * self.dt
    def advance_time(self,
                     n_samples):
        self.chirpt += float(n_samples) * self.dt
        if self.downconversion_filter == "cic":
            self.cic_integrator_state[:] = 0
            self.cic_comb_state[:] = 0


def chirp(L, f0=-12.5e6, cr=100e3, sr=25e6):
    """
    Generate a chirp.
    """
    tv = n.arange(L, dtype=n.float64) / sr
    dphase = 0.5 * tv**2 * cr * 2 * n.pi
    chirpv = n.array(n.exp(1j * n.mod(dphase, 2 * n.pi)) *
                     n.exp(1j * 2.0 * n.pi * f0 * tv), dtype=n.complex64)
    return (chirpv)


if __name__ == "__main__":
    cdc = chirp_downconvert(dec=2500)
    # test. this should downconvert to a DC signal
    z_in = chirp(L=25000000 + 5000)
    z_out = n.zeros(1000, dtype=n.complex64)
    import time
    cput0 = time.time()
    cdc.consume(z_in, z_out, 1000)
    cput1 = time.time()
    print((cput1 - cput0) / 0.1)
    import matplotlib.pyplot as plt
    plt.plot(z_out.real)
    plt.plot(z_out.imag)
    plt.show()
