import imageio
import matplotlib.pyplot as plt
import glob
import sys
import h5py
import os
import numpy as n

# Label ionogram manually using a mouse and a keyboard

# if true, plot all scaled ionograms. otherwise scale only unscaled ionograms, skipping already labeled ionograms
review=False

data_dir="/data1/noire/noire/ski/2022-05-02"
fl=glob.glob("%s/lfm*.h5"%(data_dir))
fl.sort()

def normalize(S):
    for fi in range(S.shape[0]):
        noise_floor=n.median(S[fi,:])
        std_floor=n.median(n.abs(S[fi,:]-noise_floor))
        S[fi,:]=(S[fi,:]-noise_floor)/std_floor
    S[S<0]=1e-3
    return(S)

for fi,f in enumerate(fl):
    hin=h5py.File(f,"a")
    if "fof2" in hin.keys():
        print("skipping %s"%(f))
        hin.close()
        continue
    
    print(hin.keys())

    S=normalize(hin["S"][()])
    freqs=hin["freqs"][()]
    ranges=hin["ranges"][()]

    fig, ax = plt.subplots(figsize=(18,12))    

    fof2=0.0
    hf=0.0
    fe=0.0
    he=0.0
    
    if "hf" in hin.keys():
        hf=hin["hf"][()]
        ax.axhline(hf,color="green")
        fig.canvas.draw()
        
    if "fof2" in hin.keys():
        fof2=hin["fof2"][()]
        ax.axvline(fof2,color="red")
        fig.canvas.draw()
        
        
    if "fe" in hin.keys():
        fe=hin["fe"][()]
        ax.axvline(fe,color="blue")
        fig.canvas.draw()
        
        
    if "he" in hin.keys():
        he=hin["he"][()]
        ax.axhline(he,color="white")
        fig.canvas.draw()

    dB=10.0*n.log10(n.transpose(S))
    dB[dB<-3]=-3
    dB[dB>20]=20    
    

    def press(event):
        global fof2, hf, he, fe
        x, y = event.xdata, event.ydata
        print("press %f %f"%(x,y))
        sys.stdout.flush()
        if event.key == '1':
            fof2=x
            ax.axvline(fof2,color="red")
            fig.canvas.draw()
            
        if event.key == '2':
            hf=y
            ax.axhline(hf,color="green")
            fig.canvas.draw()
            
        if event.key == '3':
            fe=x
            ax.axvline(fe,color="blue")
            fig.canvas.draw()
            
        if event.key == '4':
            he=y
            ax.axhline(he,color="white")
            fig.canvas.draw()
            
#        if event.key == '5':
#            he=0.0
 #           fe=0.0
  #          hmf=0.0
   #         fof2=0.0
    #        fig.canvas.draw()
            
        if event.key == '9':
            print("saving %s %f %f %f %f"%(f,fof2,hf,fe,he))
            if "fof2" not in hin.keys():
                hin["fof2"]=fof2
            if "hmf" not in hin.keys():                
                hin["hmf"]=hf
            if "fe" not in hin.keys():                
                hin["fe"]=fe
            if "he" not in hin.keys():                
                hin["he"]=he
            hin.close()
            plt.close()
            
        if event.key == '0':            
            print("skiping %s"%(f))
            hin.close()
            plt.close()
            
    fig.canvas.mpl_connect('key_press_event', press)
    ax.pcolormesh(freqs,ranges,dB)
    plt.title("%d/%d\n1) fof2 2) hf 3) fe 4) he 9) save 0) skip"%(fi,len(fl)))
    plt.show()

    try:
        hin.close()
    except:
        print("couldn't close h5 file. maybe already closed")
