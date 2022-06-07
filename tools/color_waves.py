import matplotlib.pyplot as plt
import matplotlib
import numpy as n
import h5py
from matplotlib.patches import Patch

fl=["Ivalo.h5","Kuusamo.h5","Oulu.h5","Skibotn.h5"]
names=["Ivalo","Kuusamo","Oulu","Skibotn"]
colormaps=["Reds","Greens","Oranges","Blues"]

legend_elements=[]
for fi,f in enumerate(fl):
    hi=h5py.File(f,"r")


    iv=10.0*n.log10(n.transpose(hi["S1"]))
#    iv=n.transpose(hi["S1"])
    iv[iv<0]=0.0
    iv[iv>20]=20.0
    alpha_iv=n.copy(iv)
    alpha_iv=alpha_iv/20.0
    alpha_iv[iv>1]=1.0
    alpha_iv[iv<0]=0.0
    
    plt.pcolormesh(hi["thour"][()],hi["freq"][()]+fi,iv,cmap=colormaps[fi],vmin=0,vmax=20,alpha=alpha_iv)
    cmap=matplotlib.cm.get_cmap(colormaps[fi])
    legend_elements.append(Patch(facecolor=cmap(0.75),label="%s + %d MHz"%(names[fi],fi)))
    hi.close()

norm=matplotlib.colors.Normalize(vmin=0,vmax=20)
sm=plt.cm.ScalarMappable(cmap="Greys",norm=norm)
sm.set_array([])
cb=plt.colorbar(sm)
cb.set_label("SNR (dB)")
plt.legend(handles=legend_elements)
plt.xlabel("Time (hours)")
plt.ylabel("Frequency (MHz)")

plt.title("2022-05-02 - F-region range cut")
plt.ylim([5,11])
plt.xlim([10,20])

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


plt.show()





    
