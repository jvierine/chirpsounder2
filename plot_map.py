import matplotlib
#matplotlib.use("Agg")  # safe for headless
import numpy as n
import matplotlib.pyplot as plt
import chirp_config as cc
import cartopy.crs as ccrs
import cartopy.feature as cfeature

import argparse

def plot_map(conf, set_extent=True, ofname="map.png"):
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
    if set_extent:
        ax.set_extent([-10, 40, 45, 90])

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

    # Filter for DOB and TGO only
    target_stations = {"DOB", "TGO"}
    filtered_links = [l for l in conf.station_links if l[0] in target_stations or l[1] in target_stations]
    stations_to_plot = target_stations.copy()
    
    # Add stations that connect to DOB or TGO
    for l in filtered_links:
        stations_to_plot.add(l[0])
        stations_to_plot.add(l[1])
    
    # Plot links
    for l in filtered_links:
        print(l)
        lons=n.linspace(conf.station_info[l[0]]["lon"],conf.station_info[l[1]]["lon"],num=50)
        lats=n.linspace(conf.station_info[l[0]]["lat"],conf.station_info[l[1]]["lat"],num=50)

        # Determine link color based on destination
        if l[1] == "TGO" or l[0] == "TGO":
            color = "orange"
        elif l[1] == "DOB" or l[0] == "DOB":
            color = "green"
        else:
            color = "black"
        
        ax.plot(lons,lats,
                color=color,transform=ccrs.PlateCarree())
    # Plot stations
    for name, s in stations.items():
        if name in stations_to_plot:
            lat = s["lat"]
            lon = s["lon"]

            ax.plot(
                lon, lat,
                marker='o',
                transform=ccrs.PlateCarree(),
                label=s["name"]
            )


    plt.legend()
    plt.title("TGO Ionospheric Sounding Network")

    plt.savefig(ofname, dpi=150)  # save BEFORE show
    plt.show()

    print("Saved")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Realtime digisonde ionogram")
    parser.add_argument(
        "--config",
        type=str,
        default="examples/marieluise/tgo.ini",
        help="Path to configuration file"
    )
    args = parser.parse_args()

    plot_map(cc.chirp_config(args.config),set_extent=False,ofname="map_all.png")
    plot_map(cc.chirp_config(args.config),set_extent=True,ofname="map_scand.png")
