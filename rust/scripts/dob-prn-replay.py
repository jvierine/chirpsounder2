#!/usr/bin/env python3
"""Compare identical bounded carrier samples before replacing a live binary."""
import hashlib
import json
import pathlib
import re
import resource
import subprocess
import sys
import time
import h5py
import numpy as np

root = pathlib.Path("/data0/recorder-health/dob-performance") / sys.argv[1]
root.mkdir(parents=True, exist_ok=True)
profile, old, new, feed = sys.argv[2:]
c = json.loads(pathlib.Path(profile).read_text())
c["data_dir"] = "/dev/shm/hf25" if feed == "live25" else f"/data0/recorder-health/dob50-probe/{feed}"
epoch = str(int(time.time()) - 10) if feed == "live25" else "1794662813"
c["frequencies_hz"] = [c["frequencies_hz"][0]]
c["code_indices"] = [c["code_indices"][0]]
c["retain_candidate_snr"] = True
times = []
cpu_times = []
stage_times = []
outputs = []
for label, binary in (("old", old), ("new", new)):
    out = root / label
    if out.exists() and any(out.iterdir()):
        raise FileExistsError(f"Choose a fresh replay label: {out}")
    out.mkdir(exist_ok=True)
    c["output_dir"] = str(out)
    config = root / f"{label}.json"
    config.write_text(json.dumps(c))
    start = time.monotonic()
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    result = subprocess.run(["nice", "-n", "19", binary, str(config), "--once", "--start", epoch], capture_output=True, text=True)
    times.append(time.monotonic() - start)
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu_times.append(after.ru_utime + after.ru_stime - before.ru_utime - before.ru_stime)
    stage = re.search(r"read ([0-9.]+)s, channel ([0-9.]+)s, correlate ([0-9.]+)s, finish ([0-9.]+)s", result.stderr)
    stage_times.append([float(v) for v in stage.groups()] if stage else [float("nan")] * 4)
    print(label, result.stderr, flush=True)
    result.check_returncode()
    outputs.append(next(out.glob("*/*.h5")))
with h5py.File(outputs[0]) as oldf, h5py.File(outputs[1]) as newf:
    keys = ["frequency_complete", "code_identity_margin_db", "SNR_sparse_indices", "SNR_sparse_values"]
    if "candidate_snr" in oldf and "candidate_snr" in newf:
        keys.append("candidate_snr")
    for key in keys:
        a, b = oldf[key][...], newf[key][...]
        np.testing.assert_allclose(a, b, rtol=2e-5, atol=2e-5, equal_nan=True, err_msg=key)
with h5py.File(root / "comparison.h5", "w") as f:
    f["process_wall_seconds_old_new"] = times
    f["process_cpu_seconds_old_new"] = cpu_times
    f["read_channel_correlate_finish_seconds_old_new"] = stage_times
    f.attrs["sample_epoch"] = epoch
    f.attrs["old_binary"] = old
    f.attrs["new_binary"] = new
    f.attrs["old_binary_sha256"] = hashlib.sha256(pathlib.Path(old).read_bytes()).hexdigest()
    f.attrs["new_binary_sha256"] = hashlib.sha256(pathlib.Path(new).read_bytes()).hexdigest()
    f.attrs["profile"] = profile
    f.attrs["samples_agree_rtol_atol"] = 2e-5
print("Archived carrier results agree; process seconds old/new:", times, flush=True)
