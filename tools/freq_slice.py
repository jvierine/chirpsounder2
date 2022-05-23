
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

f0 = 5.0
S0=n.zeros([len(fl),650])
for fi,f in enumerate(fl):
    try:
        h=h5py.File(f,"r")
        print(h.keys())
#        S=normalize(h["S"][()])
        S=h["S"][()]
        fr=h["freqs"][()]/1e6
        fidx=n.argmin(n.abs(f0-fr))
        print(S.shape)
        print(fidx)
        S0[fi,:]=S[fidx,:]/n.median(n.abs(S[fidx,:]))
        h.close()
        print(f)
    except:
        pass
        h.close()


plt.pcolormesh(n.transpose(S0),vmin=0,vmax=20)
plt.colorbar()
plt.show()
