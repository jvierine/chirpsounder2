import numpy as n
import matplotlib.pyplot as plt
import glob
import h5py


fl=glob.glob("/data1/see/2025-11-20/see*.h5")
fl.sort()
print(len(fl))

h=h5py.File(fl[0],"r")
S=h["S"][()]
f=n.fft.fftshift(n.fft.fftfreq(250000,d=1/25e6))+12.5e6
t=n.arange(len(fl))
print(len(S))
print(h.keys())

h.close()
print(len(fl))

nt=1800
dt=int(len(fl)/nt)
tidx=n.arange(0,len(fl),dt)
nt=len(tidx)
Sm=n.zeros([len(S),nt])
for i,ti in enumerate(tidx):
    h=h5py.File(fl[ti],"r")
    Sm[:,i]=n.fft.fftshift(h["S"][()])
    h.close()

fdec=100
idx=n.arange(0,250000,100)
S0=Sm[idx,:]
S0[:,:]=0
for i in range(fdec):
    S0+=Sm[idx+i,:]
dB=10.0*n.log10(S0)
vmin=n.median(dB)
plt.figure(figsize=(10,8))
plt.pcolormesh(n.arange(nt),f[idx]/1e6,dB,vmin=vmin-6,vmax=vmin+20,cmap="plasma")
plt.ylabel("Frequency (MHz)")
plt.xlabel("Time (s)")
plt.colorbar()
plt.show()

