
import glob
import matplotlib.pyplot as plt
import h5py
import numpy as n

def normalize(S):
    for fi in range(S.shape[0]):
        noise_floor=n.median(S[fi,:])
        std_floor=n.median(n.abs(S[fi,:]-noise_floor))
        S[fi,:]=(S[fi,:]-noise_floor)/std_floor
    S[S<0]=1e-3
    return(S)

data_dir="/data1/noire/noire/ski/2022-05-02"
fl=glob.glob("%s/lfm*.h5"%(data_dir))
fl.sort()


fof2=[]
hmf=[]
t=[]

# plot this frequency
f0 = 5.0
# plot this range
r0 = 950e3
S0=n.zeros([len(fl),650])
S1=n.zeros([len(fl),310])

h=h5py.File(fl[0],"r")

print(h.keys())
fr=n.copy(h["freqs"][()]/1e6)
ra=n.copy(h["ranges"][()])
fidx=n.argmin(n.abs(f0-fr))
ridx=n.argmin(n.abs(r0-ra))
h.close()        

for fi,f in enumerate(fl):
    try:
        h=h5py.File(f,"r")
        print(h.keys())
        S=normalize(h["S"][()])
        S0[fi,:]=S[fidx,:]#/n.median(n.abs(S[fidx,:]))
        S1[fi,:]=S[:,ridx]#/n.median(n.abs(S[:,ridx]))
        h.close()
    #    print(f)
    except:
        S0[fi,:]=1e-3
        S1[fi,:]=1e-3        
        pass
        h.close()
S0[S0<0]=1e-3
S1[S1<0]=1e-3

plt.pcolormesh(10.0*n.log10(n.transpose(S0)),vmin=0,vmax=20)
plt.colorbar()
plt.show()
plt.pcolormesh(10.0*n.log10(n.transpose(S1)),vmin=0,vmax=20)
plt.colorbar()
plt.show()
