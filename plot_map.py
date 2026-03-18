import matplotlib
#matplotlib.use("Agg")  # safe for headless
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
    fig = plt.figure(figsize=(10, 8),constrained_layout=True)

    proj = ccrs.LambertConformal(
    central_longitude=15,
    central_latitude=65,
    standard_parallels=(55, 75)
)
    ax = plt.axes(projection=proj)

    # Add basic features
    ax.add_feature(cfeature.COASTLINE)
    ax.add_feature(cfeature.BORDERS, linestyle=':')
    ax.add_feature(cfeature.LAND, alpha=0.3)
    ax.add_feature(cfeature.OCEAN,alpha=0.3)

    # Set extent
    #ax.set_extent([-10, 40, 50, 80])

    # ✅ Add gridlines
    gl = ax.gridlines(
        crs=ccrs.PlateCarree(),
        draw_labels=True,
        linewidth=0.8,
        linestyle='--',
        alpha=0.6,
        zorder=10
    )

    # Customize labels
    gl.top_labels = False
    gl.right_labels = False
    gl.xlabel_style = {'size': 10}
    gl.ylabel_style = {'size': 10}

    # Plot stations
    for name, s in stations.items():
        lat = s["lat"]
        lon = s["lon"]

        ax.plot(
            lon, lat,
            marker='o',
            transform=ccrs.PlateCarree(),
            label=s["name"]
        )
    for l in conf.station_links:
        ax.plot([conf.station_info[l[0]]["lon"],conf.station_info[l[1]]["lon"]],
                [conf.station_info[l[0]]["lat"],conf.station_info[l[1]]["lat"]],
                color="black",transform=ccrs.PlateCarree())


    plt.legend()
    plt.title("TGO Ionospheric Sounding Network")

    plt.savefig("map.png", dpi=150)  # save BEFORE show
    plt.show()

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
