#!/usr/bin/env python


import numpy as n
import matplotlib.pyplot as plt
import glob
import re

fl=glob.glob("mf*.bin")
freq=n.array([])
t0s=n.array([])
crs=n.array([])


#                header.tofile(fo)
 #               pf.tofile(fo)
  #              cf.tofile(fo)
   #             ff.tofile(fo)
    #            if0.tofile(fo)
     #           fo.flush()


for f in fl:
    print(f)
    a=n.fromfile(f,dtype=[("h",n.float64),("p",n.float64),("cf",n.float64),("ff",n.float64),("i",n.int64)])
    gidx=n.where(a["i"] > 0)[0]
    freq=n.concatenate((freq,a["ff"][gidx]))
    t0s=n.concatenate((t0s,a["i"][gidx]))
    print(a["i"])
    crs=n.concatenate((crs,a["cf"][gidx]))
                      
cru=n.unique(crs)
print(cru)
#cru=cru[n.where(cru != 0)[0]]

gidx=n.where( (crs != 0))[0]
freq=freq[gidx]
t0s=t0s[gidx]
crs=crs[gidx]

plt.style.use("dark_background")
colors=["red","blue","green","orange","yellow"]
for ci,c in enumerate(cru):
    idx=n.where(crs == c)[0]
    if c == 50e3:
        plt.scatter(freq[idx]/1e6,(t0s[idx]-n.min(t0s))/20e6/3600.0/24.0,c=colors[ci],edgecolors='none',label="%d"%(c/1e3),alpha=0.5)
    else:
        plt.scatter(freq[idx]/1e6,(t0s[idx]-n.min(t0s))/20e6/3600.0/24.0,c=colors[ci],edgecolors='none',label="%d"%(c/1e3),alpha=0.5)
    
plt.title("Chirp sounder detections")
plt.ylabel("Days")
plt.xlabel("Frequency (MHz)")    
plt.xlim([0,20])
leg=plt.legend()
for lh in leg.legendHandles:
    lh.set_alpha(1.0)
plt.show()
