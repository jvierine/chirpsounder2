"""Propagation-distance helpers for chirpsounder plots.

The functions here intentionally avoid plotting dependencies so they can be
used by detection plots, ionogram plots, digisondes, and future LFM sounders.
"""

from __future__ import annotations

import math


EARTH_RADIUS_KM = 6371.0088


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
) -> float:
    """Estimate one-way virtual distance from transmitter/receiver coordinates."""
    return propagation_factor * great_circle_distance_km(
        transmitter["lat"],
        transmitter["lon"],
        receiver["lat"],
        receiver["lon"],
    )


def auto_propagation_bands(
    station_info: dict,
    receiver_name: str,
    transmitter_names: list[str] | tuple[str, ...],
    propagation_factor: float | str = "auto",
    fractional_half_width: float = 0.15,
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
    receiver = station_info[receiver_name]
    bands = []
    for tx_name in transmitter_names:
        if tx_name not in station_info:
            continue
        tx = station_info[tx_name]
        center = virtual_distance_km(tx, receiver, propagation_factor)
        half_width = max(100.0, center * float(fractional_half_width))
        bands.append(
            {
                "name": tx_name,
                "label": tx.get("name", tx_name),
                "center_km": center,
                "min_km": max(0.0, center - half_width),
                "max_km": center + half_width,
                "distance_km": great_circle_distance_km(
                    tx["lat"], tx["lon"], receiver["lat"], receiver["lon"]
                ),
            }
        )
    return bands
