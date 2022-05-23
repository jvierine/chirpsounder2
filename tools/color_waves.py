import matplotlib.pyplot as plt
import numpy as n
import h5py

fl=["Ivalo.h5","Kuusamo.h5","Oulu.h5","Skibotn.h5"]
colormaps=["Reds","Greens","Oranges","Blues"]


for fi,f in enumerate(fl):
    hi=h5py.File(f,"r")


    iv=10.0*n.log10(n.transpose(hi["S1"]))
    alpha_iv=n.copy(iv)
    alpha_iv=alpha_iv/20.0
    alpha_iv[iv>1]=1.0
    alpha_iv[iv<0]=0.0
    
    plt.pcolormesh(hi["thour"][()],hi["freq"][()]+fi,iv,cmap=colormaps[fi],vmin=0,vmax=20,alpha=alpha_iv,label="%s + %d MHz"%(f,fi))
    hi.close()
plt.colorbar()
plt.xlabel("Time (hours)")
plt.ylabel("Frequency (MHz)")
plt.legend()
plt.show()

for fi,f in enumerate(fl):
    hi=h5py.File(f,"r")


    iv=10.0*n.log10(n.transpose(hi["S0"]))
    alpha_iv=n.copy(iv)
    alpha_iv=alpha_iv/20.0
    alpha_iv[iv>1]=1.0
    alpha_iv[iv<0]=0.0
    
    plt.pcolormesh(hi["thour"][()],hi["ranges"][()],iv,cmap=colormaps[fi],vmin=0,vmax=20,alpha=alpha_iv,label=f)
    hi.close()
plt.colorbar()
plt.xlabel()
plt.show()





    
