"""Export the existing receiver propagation model for the Rust/web viewer.

Run with conda run -n base python scripts/export_transmitter_bands.py PATH_TO_CHIRPSOUNDER2.
Provenance belongs here; the GUI labels only the scientific expectations.
"""
import configparser
import importlib.util
import json
from pathlib import Path
import sys

source = Path(sys.argv[1]).resolve()
spec = importlib.util.spec_from_file_location("propagation", source / "station_ops/propagation.py")
model = importlib.util.module_from_spec(spec)
spec.loader.exec_module(model)
config = configparser.ConfigParser()
config.read(source / "examples/marieluise/server.ini")
stations = json.loads(config["stations"]["station_info"])
bands = {
    receiver: model.auto_propagation_bands(
        stations, receiver, ["NIC", "JORN", "ROTHR1", "ROTHR2", "ROTHR3"],
        band_overrides={"JORN": {"paths": ["short", "long"]}},
    )
    for receiver in ["TGO", "DOB", "KHO", "W2NAF"]
}
bands["DMB"] = bands["DOB"]
(Path(__file__).resolve().parents[1] / "web/transmitters.json").write_text(
    json.dumps(bands, indent=2) + "\n"
)
