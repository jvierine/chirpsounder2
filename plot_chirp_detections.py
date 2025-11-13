import numpy as n
import h5py
import glob
import matplotlib.pyplot as plt
import stuffr

# list of chirp detections
fl=glob.glob("/data0/2025-*/chirp-cha*.h5")
fl.sort()

snrs=[]
times=[]
frequencies=[]
rates=[]
chirp_rates=[]

chirp_rate=100000.0
#chirp_rate=125000.0
for f in fl:
    h=h5py.File(f,"r")
    if True:#n.abs(h["chirp_rate"][()]-chirp_rate)<10:
        snrs.append(h["snr"][()])
        chirp_rates.append(h["chirp_rate"][()])        
        times.append(h["i0"][()]/25000000)
        frequencies.append(h["f0"][()]/1e6)
        print(h.keys())
    

    h.close()

times=n.array(times)
times_dt = times.astype('datetime64[s]')
plt.scatter(times_dt,frequencies,c=10.0*n.log10(snrs))
# started to switch from loop to dipole
#plt.axvline(stuffr.date2unix(2025,11,4,9,20,0),color="red")
# dipole on
#plt.axvline(stuffr.date2unix(2025,11,4,9,50,0),color="red")
# back to loop
#plt.axvline(stuffr.date2unix(2025,11,4,10,30,0),color="red")

cb=plt.colorbar()
cb.set_label("SNR (dB)")
plt.xlabel("Time (UTC)")
plt.ylabel("Frequency (MHz)")
plt.show()

chirp_rates=n.array(chirp_rates,dtype=int)
frequencies=n.array(frequencies)

idx=n.where(chirp_rates==100000)[0]
plt.plot(times_dt[idx],frequencies[idx],".",label="100 kHz/s")
idx=n.where(chirp_rates==125000)[0]
plt.plot(times_dt[idx],frequencies[idx],".",label="125 kHz/s")
plt.legend()
cb.set_label("SNR (dB)")
plt.xlabel("Time (UTC)")
plt.ylabel("Frequency (MHz)")
plt.show()
