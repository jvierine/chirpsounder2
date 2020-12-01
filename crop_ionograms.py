import numpy as n
import glob
import matplotlib.pyplot as plt
import h5py
import imageio
import chirp_config as cc
import sys
import scipy.constants as c
import os

def create_cropped_ionograms(conf,
                             range_offset=300.0):

    """
    the purpose of this hacked together script is to convert ionograms into format
    suitable for scaling
    """
    print(conf.output_dir)
    fl=glob.glob("%s/*/lfm*.h5"%(conf.output_dir))
    fl.sort()

    max_dB=30.0
    min_dB=0.0
    for f in fl:
        print(f)
        h=h5py.File(f,"r")
        ranges=n.copy(h["ranges"])
        freqs=n.copy(h["freqs"])

        t0=h["t0"].value
        img_fname="dl_dataset/iono-%d.png"%(t0)
        if os.path.exists(img_fname):
            print("already exists %s. skipping"%(img_fname))
            h.close()
            continue
        

        dt=(t0-n.floor(t0))
        dr=dt*c.c/1e3
        range_gates=dr+2*ranges/1e3
        print(dr)
        ri0=n.argmin(n.abs(range_gates-range_offset))
        S=n.transpose(n.copy(h["S"].value))
        for fi in range(S.shape[1]):
            noise_floor=n.nanmedian(S[:,fi])
            S[:,fi]=(S[:,fi]-noise_floor)/noise_floor
        
        #        plt.pcolormesh(freqs,range_gates[ri0:(ri0+200)],10.0*n.log10(S[ri0:(ri0+200)]),vmin=0,vmax=30,cmap="plasma")
        #       plt.show()
        #      if False:
        #         print(h["S"].value.shape)
        #        print(h.keys())
        dB=10.0*n.log10(S)
        dB[dB>max_dB]=max_dB
        dB[dB<min_dB]=min_dB
        dB[n.isnan(dB)]=min_dB
            
        img=dB[(ri0):(ri0+200),:]
        
        img_rgs=ranges[(ri0):(ri0+200)][::-1]/1e3+range_offset
        ho=h5py.File("dl_dataset/lut.h5","w")
        ho["img_rgs"]=img_rgs
        ho["img_freqs"]=freqs/1e6
        ho.close()
        img=img[::-1,:]
        img=n.array(255.0*img/max_dB,dtype=n.uint8)
        imageio.imwrite(img_fname,img)
        h.close()
        
if __name__ == "__main__":
    print(sys.argv[1])
    if len(sys.argv) == 2:
        conf=cc.chirp_config(sys.argv[1])
    else:
        conf=cc.chirp_config()
    create_cropped_ionograms(conf)
