"""Regression against the existing recursive C mixer; reports remain HDF5."""
from pathlib import Path
import ctypes as ct,sys,os
import h5py,numpy as np
from scipy.signal.windows import hann
root=Path(__file__).parent
if len(sys.argv)==1:
    dec=625;n=40;offset=123456
    rng=np.random.default_rng(719)
    raw=(rng.normal(size=n*dec+2*dec)+1j*rng.normal(size=n*dec+2*dec)).astype('c8')
    m=np.arange(2*dec,dtype='f4')-dec
    w=(hann(len(m))*np.sin((2*np.pi/dec)*(m+1e-6))/(np.pi*(m+1e-6))).astype('f4')
    lib=ct.CDLL('/tmp/iono-lfm-reference.dylib');func=lib.consume_recursive
    ptr=ct.c_void_p
    func.argtypes=[ct.c_double,ct.c_double,ptr,ct.c_int,ptr,ptr,ct.c_int,ct.c_int,ct.c_int,ct.c_double,ct.c_double,ptr,ct.c_int]
    out=np.zeros(n,'c8');tab=np.exp(-2j*np.pi*np.arange(8192)/8192).astype('c8')
    func(offset/25e6,1/25e6,tab.ctypes.data,8192,raw.ctypes.data,out.ctypes.data,n,dec,2*dec,-12.5e6,500008.4,w.ctypes.data,1)
    with h5py.File(root/'lfm-fixture.h5','w') as h:
        h['raw_re']=raw.real;h['raw_im']=raw.imag;h['sample_offset']=offset;h['decimation']=dec;h['n_out']=n;h['reference_re']=out.real;h['reference_im']=out.imag
    print('created LFM reference fixture')
else:
    with h5py.File(root/'lfm-fixture.h5') as a,h5py.File(root/'lfm-fixture.rust.h5') as b:
        ref=a['reference_re'][()]+1j*a['reference_im'][()];got=b['z_re'][()]+1j*b['z_im'][()]
        err=float(np.max(np.abs(ref-got))/np.max(np.abs(ref)));assert err<5e-5,err
        with h5py.File(root/'lfm-parity.h5','w') as h:h['relative_max_error']=err;h['passed']=True
        print('PASS LFM mixer relative max error',err)
