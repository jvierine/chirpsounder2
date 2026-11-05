"""Register a verified receiver GPS fix and reuse the network's propagation model.

Run locally with conda run -n base python, not on the acquisition computer.
Original measured coordinates remain in the supplied HDF5 location product.
"""
import argparse
import configparser
import importlib.util
import json
import math
from pathlib import Path

import h5py

parser = argparse.ArgumentParser()
parser.add_argument("location_h5", type=Path)
parser.add_argument("chirpsounder_source", type=Path)
args = parser.parse_args()
with h5py.File(args.location_h5, "r") as h:
    lat, lon = float(h["latitude_deg"][()]), float(h["longitude_deg"][()])
if not (math.isfinite(lat) and math.isfinite(lon) and abs(lat) <= 90 and abs(lon) <= 180):
    raise ValueError("invalid GPS coordinates")
root = Path(__file__).resolve().parents[2]
paths_file, bands_file = root / "web/paths.json", root / "web/transmitters.json"
paths = json.loads(paths_file.read_text())
paths["KREA"] = {"name": "Krea University", "lat": lat, "lon": lon}
config = configparser.ConfigParser()
config.read(args.chirpsounder_source / "examples/marieluise/server.ini")
stations = json.loads(config["stations"]["station_info"])
stations["KREA"] = paths["KREA"]
spec = importlib.util.spec_from_file_location("propagation", args.chirpsounder_source / "propagation.py")
model = importlib.util.module_from_spec(spec)
spec.loader.exec_module(model)
bands = json.loads(bands_file.read_text())
bands["KREA"] = model.auto_propagation_bands(
    stations, "KREA", ["NIC", "JORN", "ROTHR1", "ROTHR2", "ROTHR3"],
    band_overrides={"JORN": {"paths": ["short", "long"]}},
)
paths_file.write_text(json.dumps(paths, separators=(",", ":")) + "\n")
bands_file.write_text(json.dumps(bands, indent=2) + "\n")
print(f"KREA GPS: {lat:.8f}, {lon:.8f}; {len(bands['KREA'])} assumed propagation gates")
