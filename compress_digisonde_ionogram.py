#!/usr/bin/env python3

import numpy as n
import matplotlib.pyplot as plt
import h5py
import glob
import os

folder = "/data1/digisonde/2025-11-05/"

files = sorted(glob.glob(os.path.join(folder, "*.h5")))
threshold=2
    
for path in files:
    fname = os.path.basename(path).replace(".h5", ".png")
    with h5py.File(path, "r") as h:
        if "S" not in h.keys():
            print("no S in keys %s"%(path))
            continue
        S=n.array(h["S"][()],dtype=n.float32)
        fvec=h["fvec"][()]
        rvec=h["rvec"][()]


#h=h5py.File("/data1/digisonde/2025-11-05/lfm_ionogram-1762301700.00.h5","r")
                         

#print(fvec)


        SNR=n.copy(S)
        nfloors=n.zeros([2,len(fvec)],dtype=n.float32)
        for i in range(len(fvec)):
            for j in range(2):
                noise_floor = n.median(S[j,i,:])
                nfloors[j,i]=noise_floor
                SNR[j,i,:]=(S[j,i,:]-noise_floor)/noise_floor

        plt.figure(figsize=(9,4.8))
        plt.subplot(121)
        SNR[SNR<0]=1e-3
        plt.pcolormesh(10.0*n.log10(SNR[0,:,:].T),vmin=0,vmax=10)
        plt.title("Original")
        plt.colorbar()
        plt.ylim([0,500])
        plt.xlabel("Frequency (MHz)")
        plt.ylabel("Virtual range (km)")
        # set low SNR values to nan to save storage
        SNR[SNR<threshold]=n.nan

        
        #plt.show()
        fnamec="%s.compressed.h5"%(path)
        ho=h5py.File(fnamec,"w")
        ho["fvec"]=n.array(fvec,dtype=n.float32)
        ho["rvec"]=n.array(rvec,dtype=n.float32)
        ho["noise_floors"]=nfloors
        ho.create_dataset("SNR",
                          data=SNR,
                          compression="gzip",
                          compression_opts=9,
                          shuffle=True)
        ho.close()
        size1=os.path.getsize(path)
        size2=os.path.getsize(fnamec)
        print("compression %1.3f"%(size1/size2))
        
        plt.subplot(122)
        SNRP=n.copy(SNR)
        SNRP[n.isnan(SNRP)]=1e-3
        plt.pcolormesh(10.0*n.log10(SNRP[0,:,:].T),vmin=0,vmax=10)
        plt.title("Compression %1.3f"%(size1/size2))
        plt.colorbar()
        plt.xlabel("Frequency (MHz)")
        plt.ylabel("Virtual range (km)")        
        plt.ylim([0,500])
        plt.savefig(os.path.join("/home/hfrx2/Documents/compression_test/2025-11-05/",fname))
        plt.tight_layout()
        plt.close()
