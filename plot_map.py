import matplotlib
matplotlib.use("Agg")  # safe for headless
import numpy as n
import matplotlib.pyplot as plt
import chirp_config as cc
import cartopy.crs as ccrs
import cartopy.feature as cfeature

import argparse

def plot_map(conf):
    print(conf.station_info)
    stations = conf.station_info

    # Create map
    fig = plt.figure(figsize=(10, 8))
    ax = plt.axes(projection=ccrs.PlateCarree())

    # Add basic features
    ax.add_feature(cfeature.COASTLINE)
    ax.add_feature(cfeature.BORDERS, linestyle=':')
    ax.add_feature(cfeature.LAND, alpha=0.3)
    ax.add_feature(cfeature.OCEAN)

    # Set extent (auto-center around stations)
    lats = [s["lat"] for s in stations.values()]
    lons = [s["lon"] for s in stations.values()]

    margin = 5
    ax.set_extent([
        min(lons) - margin,
        max(lons) + margin,
        min(lats) - margin,
        max(lats) + margin
    ])

    # Plot stations
    for name, s in stations.items():
        lat = s["lat"]
        lon = s["lon"]

        ax.plot(lon, lat, marker='o', transform=ccrs.PlateCarree())

        ax.text(
            lon + 0.2,
            lat + 0.2,
            name,
            transform=ccrs.PlateCarree()
        )

    plt.title("Chirp Sounder Station Map")
    plt.show()
    plt.savefig("map.png", dpi=150)
    print("Saved map.png")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Realtime digisonde ionogram")
    parser.add_argument(
        "--config",
        type=str,
        default="examples/marieluise/tgo.ini",
        help="Path to configuration file"
    )
    args = parser.parse_args()

    plot_map(cc.chirp_config(args.config))
