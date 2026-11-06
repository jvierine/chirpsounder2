"""Compare the Rust decoder with the established floating-point algorithm."""
import sys,os
from pathlib import Path
import h5py
import numpy as np
from scipy.signal.windows import kaiser
# Frozen Python decoder is used only for offline parity validation.
sys.path.insert(0,str(Path(os.environ.get('CHIRPSOUNDER_ROOT',str(Path.home()/'src/chirpsounder2')))/'python'))
from digisonde_stuff import complementary_code
root=Path(__file__).parent
if len(sys.argv)==1:
    rng=np.random.default_rng(9284)
    fs=25e6;n=250000;nipp=8;off=2.75e6
    codes,_,_=complementary_code(sr=100e3,mode=3)
    raw=np.empty(n*nipp,np.complex64)
    for pi in range(nipp):
        z=np.repeat(np.roll(codes[pi*1000:(pi+1)*1000],47),250)
        phase=np.exp(2j*np.pi*off*np.arange(n)/fs)
        raw[pi*n:(pi+1)*n]=z*phase+(rng.normal(size=n)+1j*rng.normal(size=n))*.03
    h=np.sinc(2*.006*(np.arange(191)-95))*2*.006*kaiser(191,8.6);h=(h/h.sum()).astype('f4')
    decoded=[]
    for pi in range(nipp):
        phase0=np.complex64(np.exp(-2j*np.pi*off*(n/fs)*pi))
        mixed=(raw[pi*n:(pi+1)*n]*np.exp(-2j*np.pi*off*np.arange(n)/fs)*phase0).astype('c8')
        stage=mixed.reshape(-1,10).mean(axis=1)
        z=np.convolve(stage,h,'same')[::25]
        kernel=np.conj(np.fft.fft(codes[pi*1000:(pi+1)*1000]));kernel[np.abs(np.fft.fftfreq(1000,d=1/100000))>30000]=1e-99
        decoded.append(np.fft.ifft(np.fft.fft(z)*kernel).astype('c8'))
    decoded=np.array(decoded)
    power=[]
    for mode in (0,1):
        d=decoded[mode*2::4]+decoded[mode*2+1::4]
        power.append(np.max(np.abs(np.fft.fft(d,axis=0))**2,axis=0))
    with h5py.File(root/'decoder-fixture.h5','w') as f:
        f['raw_re']=raw.real;f['raw_im']=raw.imag;f['frequency_offset_hz']=off
        f['reference_decoded_re']=decoded.real.ravel();f['reference_decoded_im']=decoded.imag.ravel();f['reference_power_o']=power[0];f['reference_power_x']=power[1]
    print('wrote bounded synthetic decoder fixture')
else:
    with h5py.File(root/'decoder-fixture.h5') as a,h5py.File(root/'decoder-fixture.rust.h5') as b:
        ref=a['reference_decoded_re'][()]+1j*a['reference_decoded_im'][()]
        got=b['decoded_re'][()]+1j*b['decoded_im'][()]
        relative=float(np.max(np.abs(got-ref))/np.max(np.abs(ref)))
        assert relative<3e-5,relative
        errors=[]
        for mode in ('o','x'):
            ref=a[f'reference_power_{mode}'][()];got=b[f'power_{mode}'][()]
            error=float(np.max(np.abs(got-ref))/np.max(np.abs(ref)));assert error<5e-5,error;errors.append(error)
        with h5py.File(root/'decoder-parity.h5','w') as report:
            report['decoded_relative_max_error']=relative;report['power_relative_max_errors']=errors;report['passed']=True
        print('PASS decoder relative max error',relative,'power errors',errors)
