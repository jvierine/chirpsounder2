#!/usr/bin/env python3
"""Bounded, read-only DOB timing/cgroup snapshot; numerical results stay HDF5."""
import json
import pathlib
import re
import subprocess
import sys
import time
import h5py

assert subprocess.check_output(["hostname", "-s"], text=True).strip() == "tgo-mic1"
label = sys.argv[1]
assert re.fullmatch(r"[a-z0-9-]+", label)
units = ["iono-prn.slice", "dob50-acquisition", "iono-prn@Kiruna", "iono-prn@Uppsala", "iono-prn@Montelibretti"]
def properties(unit):
    text = subprocess.check_output(["systemctl", "--user", "show", unit,
        "-p", "MainPID", "-p", "ControlGroup", "-p", "CPUQuotaPerSecUSec"], text=True)
    return dict(line.split("=", 1) for line in text.splitlines())
props = {u: properties(u) for u in units}
def counters():
    out = {}
    for u, p in props.items():
        path = pathlib.Path("/sys/fs/cgroup") / p["ControlGroup"].lstrip("/") / "cpu.stat"
        out[u] = {k: int(v) for k, v in (line.split() for line in path.read_text().splitlines())}
    return out
before = counters()
start = time.time()
time.sleep(5)
elapsed = time.time() - start
after = counters()
root = pathlib.Path("/data0/recorder-health/dob-performance")
root.mkdir(parents=True, exist_ok=True)
path = root / f"{int(start)}-{label}.h5"
summary = {}
with h5py.File(path, "w") as f:
    f.attrs["label"] = label
    f.attrs["started_unix"] = start
    f.attrs["interval_seconds"] = elapsed
    f.attrs["units_json"] = json.dumps(props)
    for u in units:
        delta = {k: after[u][k] - before[u].get(k, 0) for k in after[u]}
        g = f.create_group(u)
        for k, v in delta.items():
            g[k] = v
        summary[u] = {k: delta.get(k, 0) for k in ("usage_usec", "nr_periods", "nr_throttled", "throttled_usec")}
        if u != "iono-prn.slice" and u != "dob50-acquisition":
            logs = subprocess.check_output(["journalctl", "--user", "-u", u, "-n", "80", "--no-pager"], text=True)
            times = [float(v) for v in re.findall(r": ([0-9.]+)s compute", logs)]
            g["recent_dwell_wall_seconds"] = times
print(json.dumps({"report": str(path), "elapsed": elapsed, "delta": summary}))
