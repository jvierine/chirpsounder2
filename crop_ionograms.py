import numpy as n
import glob
import matplotlib.pyplot as plt
import h5py
import imageio

fl=glob.glob("chirp_juha/2020-11-23/lfm*.h5")
fl.sort()

max_dB=30.0
min_dB=0.0
for f in fl:
    try:
        h=h5py.File(f,"r")
        print(h.keys())
        S=n.transpose(n.copy(h["S"].value))
        print(S.shape)
        S[n.isnan(S)]=0.0
        for fi in range(S.shape[1]):
            noise_floor=n.nanmedian(S[:,fi])
            S[:,fi]=(S[:,fi]-noise_floor)/noise_floor
            
        max_range=n.nanargmax((n.sum(S,axis=1)))
        dB=10.0*n.log10(S)
        dB[dB>max_dB]=max_dB
        dB[dB<min_dB]=min_dB
        dB[n.isnan(dB)]=min_dB
        
        img=dB[(max_range-100):(max_range+100),:]
        img=img[::-1,:]
        print(n.max(img))
        print(n.min(img))
        imageio.imwrite("dl_dataset2/iono-%d.png"%(h["t0"].value),img)
        #    plt.pcolormesh(dB[(max_range-100):(max_range+100),:])
        #   plt.colorbar()
        #  plt.show()
        h.close()
    except:
        print("error %s"%(f))
        pass
