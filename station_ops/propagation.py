"""Propagation-distance helpers for chirpsounder plots.

The functions here intentionally avoid plotting dependencies so they can be
used by detection plots, ionogram plots, digisondes, and future LFM sounders.
"""

from __future__ import annotations

import math


EARTH_RADIUS_KM = 6371.0088
EARTH_CIRCUMFERENCE_KM = 2.0 * math.pi * EARTH_RADIUS_KM


def great_circle_distance_km(lat1: float, lon1: float, lat2: float, lon2: float) -> float:
    """Return WGS84-like spherical great-circle distance in kilometers."""
    phi1 = math.radians(float(lat1))
    phi2 = math.radians(float(lat2))
    dphi = phi2 - phi1
    dlambda = math.radians(float(lon2) - float(lon1))
    a = (
        math.sin(dphi / 2.0) ** 2
        + math.cos(phi1) * math.cos(phi2) * math.sin(dlambda / 2.0) ** 2
    )
    return 2.0 * EARTH_RADIUS_KM * math.asin(min(1.0, math.sqrt(a)))


def propagation_factor_from_calibration(
    station_info: dict,
    receiver: str = "TGO",
    calibrations: tuple[tuple[str, float], ...] = (
        ("NIC", 0.5 * (3900.0 + 5500.0)),
        ("JORN", 0.5 * (13500.0 + 16900.0)),
    ),
) -> float:
    """Estimate virtual-range/great-circle factor from known manual bands."""
    factors = []
    rx = station_info[receiver]
    for tx_name, virtual_range_km in calibrations:
        tx = station_info[tx_name]
        distance_km = great_circle_distance_km(
            tx["lat"], tx["lon"], rx["lat"], rx["lon"]
        )
        if distance_km > 0.0:
            factors.append(float(virtual_range_km) / distance_km)
    if not factors:
        raise ValueError("no valid propagation calibration links")
    return sum(factors) / len(factors)


def virtual_distance_km(
    transmitter: dict,
    receiver: dict,
    propagation_factor: float,
    path: str = "short",
) -> float:
    """Estimate one-way virtual distance from transmitter/receiver coordinates."""
    distance_km = great_circle_distance_km(
        transmitter["lat"],
        transmitter["lon"],
        receiver["lat"],
        receiver["lon"],
    )
    if path == "long":
        distance_km = EARTH_CIRCUMFERENCE_KM - distance_km
    elif path != "short":
        raise ValueError("great-circle path must be 'short' or 'long'")
    return propagation_factor * distance_km


def auto_propagation_bands(
    station_info: dict,
    receiver_name: str,
    transmitter_names: list[str] | tuple[str, ...],
    propagation_factor: float | str = "auto",
    fractional_half_width: float = 0.15,
    band_overrides: dict | None = None,
) -> list[dict]:
    """Return plot bands estimated from great-circle distance.

    If ``propagation_factor`` is ``"auto"``, the factor is calibrated from the
    old TGO manual bands for Cyprus and Australia/JORN.
    """
    if receiver_name not in station_info:
        return []
    if propagation_factor == "auto":
        propagation_factor = propagation_factor_from_calibration(station_info)
    propagation_factor = float(propagation_factor)
    if band_overrides is None:
        band_overrides = {}
    receiver = station_info[receiver_name]
    bands = []
    for tx_name in transmitter_names:
        if tx_name not in station_info:
            continue
        tx = station_info[tx_name]
        override = band_overrides.get(tx_name, {})
        paths = override.get("paths", ["short"])
        band_fractional_half_width = float(
            override.get("fractional_half_width", fractional_half_width)
        )
        short_distance_km = great_circle_distance_km(
            tx["lat"], tx["lon"], receiver["lat"], receiver["lon"]
        )
        for path in paths:
            center = virtual_distance_km(tx, receiver, propagation_factor, path=path)
            half_width = max(100.0, center * band_fractional_half_width)
            label = override.get("label", tx.get("name", tx_name))
            if len(paths) > 1:
                label = "%s %s" % (label, path)
            band = {
                "name": tx_name,
                "path": path,
                "label": label,
                "center_km": center,
                "min_km": max(0.0, center - half_width),
                "max_km": center + half_width,
                "distance_km": (
                    short_distance_km
                    if path == "short"
                    else EARTH_CIRCUMFERENCE_KM - short_distance_km
                ),
            }
            if "min_km" in override and path == "short":
                band["min_km"] = float(override["min_km"])
            if "max_km" in override and path == "short":
                band["max_km"] = float(override["max_km"])
            if "center_km" in override and path == "short":
                band["center_km"] = float(override["center_km"])
            bands.append(band)
    return bands


def detection_range_limits_km(conf) -> tuple[float, float]:
    """Return configured timing-detection range limits in kilometers."""
    min_km = float(getattr(conf, "detection_range_filter_min_km", 0.0))
    max_km = getattr(conf, "detection_range_filter_max_km", "auto_jorn")
    if isinstance(max_km, str) and max_km == "auto_jorn":
        bands = auto_propagation_bands(
            conf.station_info,
            conf.station_name,
            conf.propagation_range_transmitters,
            conf.propagation_range_factor,
            conf.propagation_band_fraction,
            conf.propagation_range_band_overrides,
        )
        jorn_max = [
            float(band["max_km"])
            for band in bands
            if band.get("name") == "JORN"
        ]
        if jorn_max:
            max_km = max(jorn_max)
        else:
            max_km = 30000.0
    return min_km, float(max_km)
