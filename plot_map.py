import matplotlib
#matplotlib.use("Agg")  # safe for headless
import numpy as n
import matplotlib.pyplot as plt
import chirp_config as cc
import cartopy.crs as ccrs
import cartopy.feature as cfeature

import argparse

def plot_map(conf, set_extent=True, ofname="map.png", extent=None, target_stations=None):
    print(conf.station_info)
    stations = conf.station_info
    station_colors = {
        "TGO": "orange",
        "DOB": "green",
        "KHO": "red",
        "W2NAF": "blue",
    }
  # Create map
    fig = plt.figure(figsize=(10, 8),constrained_layout=True)

    if extent is None:
        central_longitude = 15
        central_latitude = 65
        standard_parallels = (55, 75)
    else:
        central_longitude = 0.5 * (extent[0] + extent[1])
        central_latitude = 0.5 * (extent[2] + extent[3])
        standard_parallels = (extent[2] + 5, extent[3] - 5)

    proj = ccrs.LambertConformal(
        central_longitude=central_longitude,
        central_latitude=central_latitude,
        standard_parallels=standard_parallels
    )
    ax = plt.axes(projection=proj)

    # Add basic features
    ax.add_feature(cfeature.COASTLINE)
    ax.add_feature(cfeature.BORDERS, linestyle=':')
    ax.add_feature(cfeature.LAND, alpha=0.3)
    ax.add_feature(cfeature.OCEAN,alpha=0.3)

    # Set extent
    if set_extent:
        ax.set_extent(extent or [-90, 40, 35, 90])

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
    gl.xlabel_style = {'size': 14}
    gl.ylabel_style = {'size': 14}

    # Filter for the receiver stations shown on the live dashboard.
    if target_stations is None:
        target_stations = {"DOB", "TGO", "KHO", "W2NAF"}
    filtered_links = [l for l in conf.station_links if (l[0] in target_stations or l[1] in target_stations)]
    stations_to_plot = target_stations.copy()

    # Add stations that connect to the receiver stations.
    for l in filtered_links:
        stations_to_plot.add(l[0])
        stations_to_plot.add(l[1])

    
    # Plot links
    for l in filtered_links:
        print(l)
        lons=n.linspace(conf.station_info[l[0]]["lon"],conf.station_info[l[1]]["lon"],num=50)
        lats=n.linspace(conf.station_info[l[0]]["lat"],conf.station_info[l[1]]["lat"],num=50)

        # Determine link color based on receiver station.
        color = "black"
        for station_name, station_color in station_colors.items():
            if station_name in l:
                color = station_color
                break
        
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
                label=s["name"],
                color=station_colors.get(name, None)
            )


    plt.legend(fontsize=12)
    plt.title("Ionospheric Sounding Network", fontsize=20)

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
    plot_map(
        cc.chirp_config(args.config),
        set_extent=True,
        ofname="map_scand.png",
        extent=[-10, 40, 45, 90],
        target_stations={"DOB", "TGO", "KHO"})
    plot_map(
        cc.chirp_config(args.config),
        set_extent=True,
        ofname="map_us.png",
        extent=[-130, -60, 20, 55],
        target_stations={"W2NAF"})
