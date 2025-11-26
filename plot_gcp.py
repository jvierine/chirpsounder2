"""
Plot great circle (orthodromic) paths from ROTHR (VA, TX, PR) and Australian JORN transmitters to Tromsø, Norway.

Dependencies:
  pip install cartopy pyproj matplotlib numpy

Author: ChatGPT (coordinates cited from public sources)
"""

import numpy as np
import matplotlib.pyplot as plt
import cartopy.crs as ccrs
from pyproj import Geod

# geod for WGS84 great-circle computations
geod = Geod(ellps="WGS84")

# --- Coordinates (lat, lon) ---
# ROTHR transmitter coordinates (from ROTHR brief). VA Tx, TX Tx, PR Tx:
rothr_sites = {
    "ROTHR_Virginia_Tx": (37 + 33/60 + 56.20/3600, -(76 + 15/60 + 49.94/3600)),  # 37°33'56.20"N, 76°15'49.94"W
    "ROTHR_Texas_Tx":    (27 + 31/60 + 38.50/3600, -(98 + 42/60 + 59.20/3600)),  # 27°31'38.50"N, 98°42'59.20"W
    # For Puerto Rico the Fort Allen site / nearby coordinates are used as the ROTHR PR Tx proxy:
    "ROTHR_PuertoRico_Tx": (18.00889, -66.50611),  # Fort Allen (approx). 
}

# JORN transmitters (approx. published coordinates for transmit sites)
jorn_sites = {
    "JORN_Longreach_Tx": (-23.658047, 144.145444),   # Longreach/TX-area (approx; Queensland transmitter)
    "JORN_Laverton_Tx":  (-28.317378, 122.843456),   # Laverton transmitter (WA)
    "JORN_AliceSprings_Tx": (-22.967561, 134.447937) # Harts Range/Alice Springs transmitter (NT)
}

# Destination Tromsø (city center coordinates)
tromso = (69 + 38/60 + 56.04/3600, 18 + 57/60 + 18.29/3600)  # ~69.6489 N, 18.9551 E

# combine all transmitters for plotting
transmitters = {}
transmitters.update(rothr_sites)
transmitters.update(jorn_sites)

# Function to compute intermediate great circle points (lon/lat arrays)
def great_circle_points(lon1, lat1, lon2, lat2, npoints=200):
    # pyproj.Geod.npts accepts lon/lat positions and returns intermediate points (not including endpoints)
    n = max(2, npoints)
    pts = geod.npts(lon1, lat1, lon2, lat2, n - 1)
    # geod.npts returns list of (lon, lat) for intermediate points; include endpoints
    lons = [lon1] + [p[0] for p in pts] + [lon2]
    lats = [lat1] + [p[1] for p in pts] + [lat2]
    return np.array(lons), np.array(lats)

# --- Plot setup ---
plt.figure(figsize=(14, 8))
# Use a Robinson projection (good global view)
ax = plt.axes(projection=ccrs.Robinson())
ax.set_global()
ax.coastlines(resolution='110m', linewidth=0.6)
ax.gridlines(draw_labels=False, linewidth=0.3, linestyle=':')

# Colors and styles
site_colors = {
    "ROTHR": "tab:blue",
    "JORN":  "tab:orange"
}

# Plot each great circle path
for name, (lat, lon) in transmitters.items():
    # note: our dict stores lat,lon; geod.npts expects lon,lat
    lon1, lat1 = lon, lat
    lon2, lat2 = tromso[1], tromso[0]
    lons, lats = great_circle_points(lon1, lat1, lon2, lat2, npoints=300)
    # project to PlateCarree for plotting lon/lat arrays
    if name.startswith("ROTHR"):
        style = {'linewidth': 1.2, 'linestyle': '-', 'alpha': 0.8}
        color = site_colors["ROTHR"]
    else:
        style = {'linewidth': 1.0, 'linestyle': '--', 'alpha': 0.9}
        color = site_colors["JORN"]
    ax.plot(lons, lats, transform=ccrs.Geodetic(), color=color, **style)
    # mark transmitter
    ax.plot(lon, lat, marker='o', markersize=6, transform=ccrs.PlateCarree(), color=color)
    # label
    ax.text(lon + 1.0, lat + 1.0, name.replace("_", " "), transform=ccrs.PlateCarree(),
            fontsize=8, weight='bold', color=color)

# Mark Tromsø
ax.plot(tromso[1], tromso[0], marker='*', markersize=10, transform=ccrs.PlateCarree(), color='red')
ax.text(tromso[1] + 1.0, tromso[0] + 1.0, "Tromsø", transform=ccrs.PlateCarree(), fontsize=10, weight='bold', color='red')

plt.title("Great-circle paths from ROTHR (VA, TX, PR) and JORN transmitters to Tromsø, Norway", fontsize=12)
plt.tight_layout()
plt.show()
