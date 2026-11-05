"""Validate a retained sparse PRN observation; write typed HDF5 evidence.

Run with conda run -n base python. Does not generate plots or alter source data.
"""
import argparse
from pathlib import Path
import h5py
import numpy as np


def validate(source, output):
    with h5py.File(source) as h:
        frequencies = h["freqs"][:]
        ranges = h["ranges"][:]
        indices = h["SNR_sparse_indices"][:].astype(np.int64)
        snr = h["SNR_sparse_values"][:]
        margins = h["code_identity_margin_db"][:]
        threshold = float(h["snr_threshold_db"][()])
        code_threshold = float(h["code_identity_required_db"][()])
        assert len(indices) == len(snr)
        assert np.all(np.diff(indices) > 0)
        assert np.all((indices >= 0) & (indices < len(frequencies) * len(ranges)))
        assert np.all(np.isfinite(snr) & (snr >= 10 ** (threshold / 10)))
        fi, ri = indices // len(ranges), indices % len(ranges)
        assert np.all(margins[fi] >= code_threshold)
        assert np.all(h["frequency_complete"][:][fi] != 0)
        with h5py.File(output, "w") as result:
            result["frequency_hz"] = frequencies[fi]
            result["virtual_path_km"] = ranges[ri] / 1000
            result["correlation_snr_db"] = 10 * np.log10(snr)
            result["code_identity_margin_db"] = margins[fi]
            result.attrs["source"] = str(Path(source).resolve())
            result.attrs["sounding_start_unix"] = float(h["t0"][()])
            result.attrs["snr_threshold_db"] = threshold
            result.attrs["code_identity_required_db"] = code_threshold
            result.attrs["complete_frequency_count"] = int(h["frequency_complete"][:].sum())
        print(f"Validated {len(indices)} sparse cells from {int(h['frequency_complete'][:].sum())} frequency dwells")


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("source")
    p.add_argument("output")
    args = p.parse_args()
    validate(args.source, args.output)
