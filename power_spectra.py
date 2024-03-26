#!/usr/bin/env python3

import numpy as n
import matplotlib.pyplot as plt
import pyfftw
import sys
import chirp_config as cc
import h5py
import digital_rf as drf
import time
import scipy.signal as ss
import chirp_det as cd
import traceback
import os


def fft(z, l=None):
    """
    wrap fft, so that it can be configured
    """
    if l == None:
        l = len(z)
    return (pyfftw.interfaces.numpy_fft.fft(z, l, planner_effort='FFTW_ESTIMATE'))


def ifft(z, l=None):
    """
    wrap fft, so that it can be configured
    """
    if l == None:
        l = len(z)
    return (pyfftw.interfaces.numpy_fft.ifft(z, l, planner_effort='FFTW_ESTIMATE'))
#    return(sf.ifft(z,l))


if __name__ == "__main__":
    if len(sys.argv) == 2:
        conf = cc.chirp_config(sys.argv[1])
    else:
        print("give configuration file as command line argument")

    sr = 25e6
    fftlen = 1024 * 2 * 2 * 2 * 2 * 2 * 2 * 2
    print("fftlen %d freqeuency resolution %f (kHz)" %
          (fftlen, sr / fftlen / 1e3))
    wfun = n.array(ss.hann(fftlen), dtype=n.float32)
    S = n.zeros(fftlen, dtype=n.float32)

    dt = int(sr) * 60
    nfft = int(n.floor(dt / fftlen))

    d = drf.DigitalRFReader(conf.data_dir)
    b = d.get_bounds(conf.channel)
    i0 = b[1] - dt - int(sr)

    while True:
        d = drf.DigitalRFReader(conf.data_dir)
        b = d.get_bounds(conf.channel)
        n_windows = int(n.floor((b[1] - i0) / dt))
        for wi in range(n_windows):
            t0 = time.time()
            S[:] = 0.0
            for fi in range(nfft):
                F = fft(wfun * d.read_vector_c81d(wi * dt +
                        fi * fftlen + i0, fftlen, conf.channel))
                S += (F * n.conj(F)).real
            t1 = time.time()
            print(t1 - t0)
            print("done")

            try:
                t0 = i0 / sr
                dname = "%s/%s" % (conf.output_dir, cd.unix2dirname(t0))
                if not os.path.exists(dname):
                    os.mkdir(dname)
                ofname = "%s/spec-%s-%1.2f.h5" % (dname, conf.station_name, t0)
                print("Writing to %s" % ofname)
                ho = h5py.File(ofname, "w")
                ho["spec"] = S
                ho["nfft"] = nfft
                ho["f0"] = conf.center_freq
                ho["sr"] = conf.sample_rate
                ho["dt"] = dt
                ho["station_name"] = conf.station_name
                ho["ch"] = conf.channel
                ho.close()
            except:
                traceback.print_exc(file=sys.stdout)
                print("error writing file")

        i0 = i0 + n_windows * dt

        b = d.get_bounds(conf.channel)
        if (b[1] - i0) < dt:
            print("sleeping for more data %d" % (b[1] - i0))
            time.sleep(1)
