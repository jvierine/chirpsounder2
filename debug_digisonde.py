import numpy as n
import digital_rf as drf
import scipy.signal as ss
import matplotlib.pyplot as plt
import digisonde_stuff as ds
import datetime
import time
import chirp_det as cd
import h5py
import os
#
# Simple simple digisonde receiver. 
# 
# Juha Vierinen (2025)
#
def unix2date(unix_seconds):
    date_string = datetime.datetime.utcfromtimestamp(unix_seconds).isoformat() + "Z"
    return(date_string)

def decimate_average(x: n.ndarray, N: int, average: bool = True) -> n.ndarray:
    """
    fast integrate and decimate. poor frequency response
    """
    # Ensure the length is a multiple of N by trimming excess samples
    trim_len = x.shape[-1] - (x.shape[-1] % N)
    if trim_len != x.shape[-1]:
        x = x[..., :trim_len]

    # Reshape and integrate (or average)
    new_shape = x.shape[:-1] + (-1, N)
    x_reshaped = x.reshape(new_shape)

    if average:
        return x_reshaped.mean(axis=-1)
    else:
        return x_reshaped.sum(axis=-1)
    
def fir_sinc_lowpass(numtaps, cutoff, beta=8.6):
    """
    Windowed-sinc lowpass FIR.
    cutoff: normalized (0..0.5) relative to input sample rate.
    """
    n_idx = n.arange(numtaps) - (numtaps - 1) / 2.0
    h = 2.0 * cutoff * n.sinc(2.0 * cutoff * n_idx)
    h *= n.kaiser(numtaps, beta)
    h /= h.sum()
    return h

def fir_decimate(x, R, h):
    """Filter then decimate by R."""
    y = n.convolve(x, h, mode='same')
    return y[::R]

# precalculate the fir window. nyquist is (0.5/50)
# aim for 0.25 Nyquist (for 100 kHz, which is 50 kHz bandwidth
h = fir_sinc_lowpass(numtaps=161, cutoff=(0.5 / 50) * 0.25)

def decimate_25_then_fir10(x):
    """
    Two-stage decimation:
      1. Decimate by 25 using simple averaging
      2. FIR lowpass filter and decimate by 10
    Total decimation = 250
    """
    # --- Stage 1: average decimation by 25 ---
    y1 = decimate_average(x, 25)

    # --- Stage 2: FIR + decimate by 10 ---
    # The cutoff must be < 0.5 / 10 (normalized to stage1 rate)
    
    y2 = fir_decimate(y1, 10, h)

    return y2

def decimate_5_then_fir50(x):
    """
    Two-stage decimation:
      1. Decimate by 5 using block averaging
      2. FIR lowpass filter + decimate by 50
    Total decimation = 250
    """
    # --- Stage 1: cheap average decimation by 5 ---
    y1 = decimate_average(x, 5)

    # --- Stage 2: FIR + decimate by 50 ---
    # cutoff < 0.5 / 50 (normalized to the stage-1 sample rate)
    
    y2 = fir_decimate(y1, 50, h)
    return(y2)
    
def calculate_ionogram(d,
                       i0,
                       dfreq=50e3,
                       freq0=1e6,                       
                       freq1=16e6,
                       n_ipp=64,
                       ipp=10000,
                       sr=25000000,
                       dec=250,
                       offset_us=-320, 
                       cf=12.5e6,
                       max_bandwidth=30e3,
                       mode=0,
                       wait_for_data=False,
                       ofname="tmp.h5"):


    # get complementary codes transmitted by digisonde
    # mode=3 includes phase flip
    cs2,codes2,modes2=ds.complementary_code(sr=100e3,mode=3)
    
    n_freq=int((freq1-freq0)/dfreq)
    ipp_dec=int(25*ipp/dec)
    
    S=n.zeros([2,n_freq,ipp_dec])
    rvec=n.arange(ipp_dec)*3.0
    fvec=n.arange(n_freq)*dfreq+freq0
    srint=int(sr/1e6)

    CS2=[]    
    n_codes=len(codes2)
    freqs=n.fft.fftfreq(srint*ipp,d=1/sr)
    freqs2=n.fft.fftfreq(ipp_dec,d=dec/sr)    
    zidx=n.where(n.abs(freqs)>max_bandwidth)[0]
    zidx2=n.where(n.abs(freqs2)>max_bandwidth)[0]    
    dec_idx=n.arange(ipp_dec)*dec

    for i in range(n_codes):
        F2=n.conj(n.fft.fft(cs2[(i*ipp_dec):(i*ipp_dec+ipp_dec)]))
        F2[zidx2]=1e-99
        CS2.append(F2)
        
    for i in range(n_freq):
        t0=time.time()

        freq=freq0+dfreq*i
        cvec=n.array(n.exp(-1j*2*n.pi*(freq-cf)*n.arange(25*ipp+1)/sr),dtype=n.complex64)
        decoded=n.zeros([n_ipp,ipp_dec],dtype=n.complex64)
     #   plt.plot(cvec[0:10000].real)
    #    plt.show()
        pha0=n.array([n.exp(1j*0)],dtype=n.complex64)
        for pi in range(n_ipp):
            start_idx=i0+(i*n_ipp+pi)*ipp*srint+srint*offset_us
            data_length=srint*ipp
            
            if wait_for_data:
                not_enough_data=True
                while not_enough_data:
                    bnow=d.get_bounds("cha")
                    if bnow[1] < (start_idx+data_length):
                        print("waiting for more data (%1.2f seconds)"%( ((start_idx+data_length)-bnow[1])/25e6 ))
                        time.sleep(1)
                    else:
                        not_enough_data=False
                
            if True:
                # it is this first step, which takes 99% of the computation.
                # multiply with complex sinusoid to shift center frequency and then low pass filter.
                # the fastest is to boxcar filter, but that has the worst frequency response
                # the slowest is to fir the whole thing at full rate, that has the best frequency response
                # compromise: average and decimate 25-> 4 MHz, and then fir decimate to 100 kHz
                # 25->1 MHz using simple boxcar filter. Then fir to improve freq response
                z=decimate_5_then_fir50(d.read_vector_c81d(i0+(i*n_ipp+pi)*ipp*srint+srint*offset_us,srint*ipp,"cha")*cvec[0:(srint*ipp)]*pha0[0])

                # deconvolve
                z=n.fft.ifft(n.fft.fft(z[0:ipp_dec])*CS2[pi%n_codes])

            if pi%4==0:
                mode=0
            if pi%4==1:
                mode=0
            if pi%4==2:
                mode=1
            if pi%4==3:
                mode=1
                
            # simple power. not going to be used, as the pulse to pulse doppler spectrum is more sensitive
            S[mode,i,:]+=n.abs(z)**2.0
            
            # save match filter output for complementary code decoding and spectral analysis
            decoded[pi,:]=z
            
            # store oscillator phase for next ipp
            pha0[0]=cvec[-1]*pha0[0]

        ipp_idx=n.arange(int(decoded.shape[0]/4))*4
        PS=n.zeros([int(decoded.shape[0]/4),decoded.shape[1]])
        for ri in range(decoded.shape[1]):
            # O-mode power specum and complementary code decode
            S[0,i,ri]=n.max(n.abs(n.fft.fft(decoded[ipp_idx,ri]+decoded[ipp_idx+1,ri]))**2.0,axis=0)
            PS[:,ri]=n.abs(n.fft.fft(decoded[ipp_idx,ri]+decoded[ipp_idx+1,ri]))**2.0
            # X-mode power specum and complementary code decode
            S[1,i,ri]=n.max(n.abs(n.fft.fft(decoded[ipp_idx+2,ri]+decoded[ipp_idx+3,ri]))**2.0,axis=0)            
        # debug spectrum
        if i==63 and False:
            plt.pcolormesh(10.0*n.log10(PS))
            plt.colorbar()
            plt.show()
            plt.pcolormesh(decoded[:,:].real)
            plt.show()
            
        t1=time.time()
        # benchmarking
        print("%d compute time %1.2f real time %1.2f, %1.2f x realtime"%(i,t1-t0,n_ipp*10e-3,(t1-t0)/(n_ipp*10e-3)))        
    SNR=n.copy(S)
    for i in range(n_freq):
        for j in range(2):
            nf=n.median(S[j,i,:])
            SNR[j,i,:]=(S[j,i,:]-nf)/nf
    SNR[SNR<0]=1e-9
    plt.pcolormesh(fvec/1e6,rvec,10.0*n.log10(SNR[0,:,:].T),vmin=-10,vmax=30)
    plt.title("Digisonde Ramfjordmoen-TGO\n%s"%( unix2date(i0/25e6)))
    plt.xlabel("Frequency (MHz)")
    plt.ylabel("One-way range (km)")
    plt.colorbar()
#    plt.subplot(122)    
 #   plt.pcolormesh(fvec/1e6,rvec,10.0*n.log10(SNR[1,:,:].T),vmin=-10,vmax=30)#,10.0*n.log10(S[1,:,:].T))
  #  plt.title("Digisonde Ramfjordmoen-TGO (X-mode)\n%s"%(unix2date(i0/25e6)))
   # plt.xlabel("Frequency (MHz)")
   # plt.ylabel("One-way range (km)")
   # plt.colorbar()
    plt.tight_layout()
    plt.savefig("%s.png"%(ofname))
    plt.close()
    ho=h5py.File(ofname,"w")
    ho["S"]=S
    ho["t0"]=i0/25e6
    ho["fvec"]=fvec
    ho["rvec"]=rvec
    ho.close()
    print("saved %s.png\nsaving %s"%(ofname,ofname))

    

def realtime_ionogram():
    
    # while true
    # -when is next ionogram
    # -if not analyzed
    #   -start analyzing ionogram with wait_for_data=True
    # -else wait
    #
    # cycle through these start delays
    d=drf.DigitalRFReader("/dev/shm/hf25/")
    b=d.get_bounds("cha")
    # next sounding (every 15 minutes)
    # look the the next sounding start coming up
    t0=15*60*25000000*n.ceil(b[1]/25000000/(15*60))
    output_dir="/data1/digisonde"
    dname="%s/%s"%(output_dir,cd.unix2dirname(t0/25e6))
    if not os.path.exists(dname):
        os.mkdir(dname)
    ofname="%s/lfm_ionogram-%1.2f.h5"%(dname,t0/25e6)
    if os.path.exists(ofname):
        print("sounding already exists. skipping")
    else:
        # estimated based on ground path ramfjordmoen-tgo (14.1 km)
        calculate_ionogram(d,
                           t0,
                           dfreq=50e3,
                           freq0=1e6,                       
                           freq1=16e6,
                           n_ipp=64,
                           ipp=10000,
                           sr=25000000,
                           dec=250,
                           offset_us=-320, 
                           cf=12.5e6,
                           wait_for_data=True,
                           ofname=ofname)

        

if __name__ == "__main__":
    while True:
        realtime_ionogram()
    d=drf.DigitalRFReader("/data1/lz1aq/hf25")
    b=d.get_bounds("cha")
    latest_sounding_start=15*60*25000000*n.ceil(b[0]/25000000/(15*60)) + 3*15*60*25000000

    # estimated based on ground path ramfjordmoen-tgo (14.1 km)
    calculate_ionogram(d,
                       latest_sounding_start,
                       dfreq=50e3,
                       freq0=1e6,                       
                       freq1=16e6,
                       n_ipp=64,
                       ipp=10000,
                       sr=25000000,
                       dec=250,
                       offset_us=-320, 
                       cf=12.5e6)

