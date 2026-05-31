import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import matplotlib.ticker as mticker
import chirp_config as cc
import cartopy.crs as ccrs
import cartopy.feature as cfeature

import argparse


def _format_lon(lon):
    if lon == 0:
        return "0 deg"
    return f"{abs(lon):g} deg{'W' if lon < 0 else 'E'}"


def _format_lat(lat):
    if lat == 0:
        return "0 deg"
    return f"{abs(lat):g} deg{'S' if lat < 0 else 'N'}"


def _add_inside_grid_labels(ax, extent, lon_step=60, lat_step=30):
    lon_min, lon_max, lat_min, lat_max = extent
    lon_values = range(
        int((lon_min + lon_step - 1) // lon_step * lon_step),
        int(lon_max) + 1,
        lon_step,
    )
    lat_values = range(
        int((lat_min + lat_step - 1) // lat_step * lat_step),
        int(lat_max) + 1,
        lat_step,
    )
    label_style = {
        "fontsize": 9,
        "color": "0.2",
        "bbox": {"facecolor": "white", "alpha": 0.55, "edgecolor": "none", "pad": 1.5},
        "clip_on": True,
        "zorder": 20,
    }
    for lon in lon_values:
        if lon_min < lon < lon_max:
            ax.text(
                lon,
                lat_min + 0.03 * (lat_max - lat_min),
                _format_lon(lon),
                ha="center",
                va="bottom",
                transform=ccrs.PlateCarree(),
                **label_style,
            )
    for lat in lat_values:
        if lat_min < lat < lat_max:
            ax.text(
                lon_min + 0.02 * (lon_max - lon_min),
                lat,
                _format_lat(lat),
                ha="left",
                va="center",
                transform=ccrs.PlateCarree(),
                **label_style,
            )


def _add_global_axes_labels(ax):
    label_style = {
        "fontsize": 9,
        "color": "0.2",
        "bbox": {"facecolor": "white", "alpha": 0.55, "edgecolor": "none", "pad": 1.5},
        "clip_on": True,
        "zorder": 20,
    }
    for x, label in [
        (0.12, "120 degW"),
        (0.30, "60 degW"),
        (0.50, "0 deg"),
        (0.68, "60 degE"),
        (0.86, "120 degE"),
    ]:
        ax.text(x, 0.05, label, ha="center", va="bottom", transform=ax.transAxes, **label_style)

    for y, label in [
        (0.19, "30 degS"),
        (0.39, "0 deg"),
        (0.59, "30 degN"),
        (0.79, "60 degN"),
    ]:
        ax.text(0.02, y, label, ha="left", va="center", transform=ax.transAxes, **label_style)


def plot_map(conf, set_extent=True, ofname="map.png", extent=None, target_stations=None):
    stations = conf.station_info
    station_colors = {
        "TGO": "orange",
        "DOB": "green",
        "KHO": "red",
        "W2NAF": "blue",
    }
    transmitter_styles = {
        "SGO": {"marker": "s", "color": "0.35"},
        "NIC": {"marker": "^", "color": "purple"},
        "Ramfjordmoen": {"marker": "v", "color": "brown"},
        "ROTHR1": {"marker": "P", "color": "tab:cyan"},
        "ROTHR2": {"marker": "X", "color": "tab:pink"},
        "ROTHR3": {"marker": "D", "color": "tab:olive"},
        "ROTHR": {"marker": "P", "color": "tab:cyan"},
        "JORN": {"marker": "*", "color": "goldenrod"},
        "Juliusruh": {"marker": "<", "color": "tab:purple"},
        "Dourbes": {"marker": ">", "color": "tab:brown"},
        "Chilton": {"marker": "h", "color": "tab:gray"},
        "DB049": {"marker": "8", "color": "tab:orange"},
    }
    transmitter_fallback_markers = ["s", "^", "v", "D", "P", "X", "*", "<", ">", "h", "8", "p"]
    transmitter_fallback_colors = [
        "0.35",
        "purple",
        "brown",
        "tab:cyan",
        "tab:pink",
        "tab:olive",
        "goldenrod",
        "tab:purple",
        "tab:brown",
        "tab:gray",
        "tab:orange",
        "tab:blue",
    ]
    global_map = not set_extent and extent is None
    # Create map
    fig = plt.figure(figsize=(12, 5.5) if global_map else (10, 8), constrained_layout=True)

    if global_map:
        extent = [-170, 170, -45, 85]
        proj = ccrs.PlateCarree()
    elif extent is None:
        central_longitude = 15
        central_latitude = 65
        standard_parallels = (55, 75)
        proj = ccrs.LambertConformal(
            central_longitude=central_longitude,
            central_latitude=central_latitude,
            standard_parallels=standard_parallels
        )
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
    ax.set_extent(extent or [-90, 40, 35, 90], crs=ccrs.PlateCarree())

    # Add gridlines and annotate coordinates inside the map frame.
    gl = ax.gridlines(
        crs=ccrs.PlateCarree(),
        draw_labels=False,
        linewidth=0.8,
        linestyle='--',
        alpha=0.6,
        zorder=10
    )
    if global_map:
        gl.xlocator = mticker.FixedLocator(range(-120, 181, 60))
        gl.ylocator = mticker.FixedLocator(range(-30, 91, 30))
        _add_global_axes_labels(ax)
    else:
        _add_inside_grid_labels(ax, extent or [-90, 40, 35, 90], lon_step=25, lat_step=10)

    # Filter for the receiver stations shown on the live dashboard.
    if target_stations is None:
        target_stations = {"DOB", "TGO", "KHO", "W2NAF"}
    filtered_links = [l for l in conf.station_links if len(l) >= 2 and l[1] in target_stations]
    stations_to_plot = target_stations.copy()

    # Add stations that connect to the receiver stations.
    for l in filtered_links:
        stations_to_plot.add(l[0])
        stations_to_plot.add(l[1])

    transmitters_to_plot = sorted(stations_to_plot - target_stations)
    for idx, station_name in enumerate(transmitters_to_plot):
        if station_name not in transmitter_styles:
            transmitter_styles[station_name] = {
                "marker": transmitter_fallback_markers[idx % len(transmitter_fallback_markers)],
                "color": transmitter_fallback_colors[idx % len(transmitter_fallback_colors)],
            }

    # Plot links
    for l in filtered_links:
        tx, rx = l[0], l[1]
        if tx not in conf.station_info or rx not in conf.station_info:
            continue

        tx_info = conf.station_info[tx]
        rx_info = conf.station_info[rx]
        color = station_colors.get(rx, "black")

        ax.plot(
            [tx_info["lon"], rx_info["lon"]],
            [tx_info["lat"], rx_info["lat"]],
            color=color,
            linewidth=1.2,
            alpha=0.8,
            transform=ccrs.Geodetic(),
        )

    # Plot stations
    for name, s in stations.items():
        if name in stations_to_plot:
            lat = s["lat"]
            lon = s["lon"]
            is_receiver = name in target_stations
            transmitter_style = transmitter_styles.get(name, {"marker": "s", "color": "0.35"})

            ax.plot(
                lon, lat,
                marker='o' if is_receiver else transmitter_style["marker"],
                linestyle='None',
                transform=ccrs.PlateCarree(),
                label=s["name"],
                color=station_colors.get(name, transmitter_style["color"]),
                markeredgecolor="black" if is_receiver else "0.15",
                markeredgewidth=0.7,
                markersize=7 if is_receiver else 6,
            )


    plt.legend(fontsize=12, loc="upper right" if global_map else "best")
    plt.title("Ionospheric Sounding Network", fontsize=20)

    plt.savefig(ofname, dpi=150)  # save BEFORE show
    plt.close(fig)

    print(f"Saved {ofname}")

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
