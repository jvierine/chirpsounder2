"""Translate an existing transmitter INI, retaining its exact frequency table.

This is deployment configuration conversion only. Live signal processing and
HDF5 production are Rust. Run with conda run -n base python.
"""
import argparse
import configparser
import hashlib
import json
import math
from pathlib import Path


def convert(source, station, transmitter, output_dir="/data0", start_offset_s=0,
            frequency_program="ini", max_receive_frequency_mhz=None):
    parser = configparser.ConfigParser(interpolation=None)
    parser.read(source)
    raw = parser["config"]
    get = lambda key: json.loads(raw[key])
    assert get("sample_rate") / get("dec") == 100000
    assert all(t == "prn" for t in get("code_type"))
    rows = get("freqs")
    if frequency_program == "ingv-generated":
        # Achille's attached iono_config.py ignores the INI freqs table and
        # generates this exact sequence from nfreqs. Do not infer this for IRF.
        count = int(get("nfreqs"))
        if not 1 <= count <= 1000:
            raise ValueError("invalid INGV frequency count")
        rows = [[(100 + j * 10) / 100, 0] for j in range(count)]
    elif frequency_program != "ini":
        raise ValueError("unknown frequency program")
    dwell = float(get("frequency_duration"))
    minutes = math.ceil(len(rows) * dwell / 60)
    if max_receive_frequency_mhz is not None:
        # Omit an inaccessible high-frequency tail, preserving the transmitter's
        # full cycle period and every retained dwell's original time offset.
        rows = [row for row in rows if row[0] <= max_receive_frequency_mhz]
        if not rows:
            raise ValueError("no frequencies within receive band")
    result = {
        "station": station, "transmitter": transmitter,
        "data_dir": "/dev/shm/hf25", "output_dir": output_dir, "channel": "ch0",
        "frequencies_hz": [row[0] * 1e6 for row in rows],
        "code_indices": [row[1] for row in rows],
        "codes": [dict(seed=get("station_id"), length=get("code_len"),
                       pulse_length=pulse, ipp=ipp, bandwidth_hz=bw,
                       control_seeds=[seed for seed in (1, 2, 3, 4) if seed != get("station_id")])
                  for pulse, ipp, bw in zip(get("pulse_length"), get("ipp"), get("bw"), strict=True)],
        "dwell_s": dwell, "period_s": 86400 / math.floor(1440 / minutes),
        "start_offset_s": start_offset_s, "range_shift_samples": get("range_shift"),
        "range_max_km": 4500,
        "snr_threshold_db": 8,
        "retain_candidate_snr": False,
        "configuration_reference": f"{Path(source).name}; sha256:{hashlib.sha256(Path(source).read_bytes()).hexdigest()}; frequency_program:{frequency_program}; schedule requires on-air verification",
    }
    return result


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("source")
    p.add_argument("station", help="receiver station identifier")
    p.add_argument("transmitter")
    p.add_argument("output")
    p.add_argument("--start-offset-s", type=float, default=0)
    p.add_argument("--frequency-program", choices=["ini", "ingv-generated"], default="ini")
    p.add_argument("--max-receive-frequency-mhz", type=float)
    args = p.parse_args()
    Path(args.output).write_text(json.dumps(convert(args.source, args.station, args.transmitter, start_offset_s=args.start_offset_s, frequency_program=args.frequency_program, max_receive_frequency_mhz=args.max_receive_frequency_mhz), indent=2) + "\n")
