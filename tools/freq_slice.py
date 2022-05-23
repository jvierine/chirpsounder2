
import glob
import matplotlib.pyplot as plt
import h5py
import numpy as n
import stuffr

def normalize(S):
    for fi in range(S.shape[0]):
        noise_floor=n.median(S[fi,:])
        std_floor=n.median(n.abs(S[fi,:]-noise_floor))
        S[fi,:]=(S[fi,:]-noise_floor)/std_floor
    S[S<0]=1e-3
    return(S)




def plot_slices(data_dir="/data1/noire/noire/oul/2022-05-02",
                name="Skitbotn"):

    fl=glob.glob("%s/lfm*.h5"%(data_dir))
    fl.sort()

    fof2=[]
    hmf=[]
    tv=[]
    
    # plot this frequency
    f0 = 5.0
    # plot this range
    r0 = 850e3
    S0=n.zeros([len(fl),650])
    S1=n.zeros([len(fl),310])
    
    
    h=h5py.File(fl[0],"r")
    
    print(h.keys())
    fr=n.copy(h["freqs"][()]/1e6)
    ra=n.copy(h["ranges"][()])
    fidx=n.argmin(n.abs(f0-fr))
    ridx=n.argmin(n.abs(r0-ra))
    h.close()        
    
    fii=0
    for fi,f in enumerate(fl):
        try:
            h=h5py.File(f,"r")
            print(h.keys())
            S=normalize(h["S"][()])
            S0[fii,:]=S[fidx,:]#/n.median(n.abs(S[fidx,:]))
            S1[fii,:]=S[:,ridx]#/n.median(n.abs(S[:,ridx]))
            tv.append(h["t0"][()])
            fii+=1
            h.close()
            #    print(f)
        except:
            h.close()
            pass
        
    S0=S0[0:fii,:]
    S1=S1[0:fii,:]
    S0[S0<0]=1e-3
    S1[S1<0]=1e-3
    tv=n.array(tv)
    thour=(tv-tv[0])/3600.0
    plt.pcolormesh(thour,ra/1e3,10.0*n.log10(n.transpose(S0)),vmin=0,vmax=20)
    plt.title("%s %s\nFrequency=%1.2f MHz"%(name,stuffr.unix2datestr(tv[0]),f0))
    plt.xlabel("Time (hour)")
    plt.ylabel("Virtual path length (km)")
    plt.colorbar()
    plt.show()
    plt.pcolormesh(thour,fr,10.0*n.log10(n.transpose(S1)),vmin=0,vmax=20)
    plt.title("%s %s\nPath length=%1.2f km"%(name,stuffr.unix2datestr(tv[0]),r0/1e3))
    plt.xlabel("Time (hour)")
    plt.ylabel("Frequency (MHz)")
    plt.colorbar()
    plt.show()


    

plot_slices(data_dir="/data1/noire/noire/ski/2022-05-02",name="Skibotn")
plot_slices(data_dir="/data1/noire/noire/iva/2022-05-02",name="Ivalo")
plot_slices(data_dir="/data1/noire/noire/oul/2022-05-02",name="Oulu")
plot_slices(data_dir="/data1/noire/noire/kuu/2022-05-02",name="Kuusamo")
