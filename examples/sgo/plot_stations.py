from mpl_toolkits.basemap import Basemap

import numpy as n
import matplotlib.pyplot as plt

sondes = [

    {"name":"Skibotn",
     "lat":69.3908,
     "lon":20.2673},
    {"name":"Oulu",
     "lat":65.086470,
     "lon":25.892159},
    
    {"name":"Kuusamo",
     "lat":65.911368,
     "lon":29.040164},

    {"name":"Ivalo",
     "lat":68.79610,
     "lon":27.88395},

    {"name":"Sodankyla",
     "lat":67.36435438251499,
     "lon":26.630436602498662}
]
lat0=sondes[4]["lat"]
lon0=sondes[4]["lon"]

m=Basemap(projection="stere",
          lat_0=lat0,
          lon_0=lon0,
          llcrnrlat=63,
          urcrnrlat=71,
          llcrnrlon=16,
          urcrnrlon=36,
          resolution="h")
m.drawcoastlines()
m.drawcountries()

pars=n.arange(62,72,2)
m.drawparallels(pars,labels=pars)

mers=n.arange(15,35,5)
m.drawmeridians(mers,labels=[1,0,0,1])


for s in sondes:
    x,y=m(s["lon"],s["lat"])
    plt.plot(x,y,".",label=s["name"],markersize=24)

plt.legend(loc=3)
plt.title("Ionosonde network")
plt.tight_layout()
plt.savefig("stations.png")
plt.show()
