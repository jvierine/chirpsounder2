import numpy as np
import h5py
import glob
import matplotlib.pyplot as plt
import argparse
import os
import sys
import stuffr  # assuming this is a custom/local module

# ------------------------------------------------------------
# Script: chirp_analysis.py
# Description:
#   Reads chirp detection HDF5 files, extracts signal parameters,
#   and visualizes frequency vs. time colored by SNR.
#   Supports multiple chirp rates and automatically formats date labels.
#   Includes a simple ASCII progress bar.
# ------------------------------------------------------------


def print_progress_bar(iteration, total, length=40):
    """Display a simple ASCII progress bar in the terminal."""
    percent = f"{100 * (iteration / float(total)):.1f}"
    filled_length = int(length * iteration // total)
    bar = "█" * filled_length + "-" * (length - filled_length)
    sys.stdout.write(f"\r|{bar}| {percent}% ({iteration}/{total})")
    sys.stdout.flush()
    if iteration == total:
        print()  # Move to a new line at the end


def main():
    # ---- Command-line arguments ----
    parser = argparse.ArgumentParser(description="Plot chirp detections from HDF5 files.")
    parser.add_argument(
        "-d", "--directory",
        type=str,
        default=".",
        help="Directory containing chirp HDF5 files (default: current directory)"
    )
    args = parser.parse_args()

    data_dir = os.path.abspath(args.directory)
    print(f"Using data directory: {data_dir}")

    # ---- Find all chirp detection files ----
    file_pattern = os.path.join(data_dir, "2*-*-*", "chirp-cha*.h5")
    fl = sorted(glob.glob(file_pattern))

    if not fl:
        print("No chirp files found. Check the directory path.")
        return

    snrs = []
    times = []
    frequencies = []
    chirp_rates = []

    total_files = len(fl)
    print(f"Found {total_files} files. Processing...\n")

    # ---- Process each file with progress bar ----
    for i, f in enumerate(fl, start=1):
        with h5py.File(f, "r") as h:
            # Extract parameters
            snrs.append(h["snr"][()])
            chirp_rates.append(h["chirp_rate"][()])
            times.append(h["i0"][()] / 25_000_000)  # Convert sample index to seconds
            frequencies.append(h["f0"][()] / 1e6)   # Convert to MHz

        print_progress_bar(i, total_files)

    # ---- Convert to NumPy arrays ----
    times = np.array(times)
    times_dt = times.astype("datetime64[s]")
    snrs = np.array(snrs)
    frequencies = np.array(frequencies)
    chirp_rates = np.array(chirp_rates, dtype=int)

    # ---- Scatter plot: Frequency vs Time ----
    plt.figure(figsize=(10, 6))
    sc = plt.scatter(times_dt, frequencies, c=10.0 * np.log10(snrs), cmap="viridis")
    cb = plt.colorbar(sc)
    cb.set_label("SNR (dB)")
    plt.xlabel("Time (UTC)")
    plt.ylabel("Frequency (MHz)")
    plt.title("Chirp Detections")

    # Improve date labels (avoid clutter)
    plt.gcf().autofmt_xdate()
    plt.tight_layout()
    plt.show()

    # ---- Separate plot by chirp rate ----
    plt.figure(figsize=(10, 6))
    unique_rates = np.unique(chirp_rates)
    for rate in unique_rates:
        idx = np.where(chirp_rates == rate)[0]
        plt.plot(times_dt[idx], frequencies[idx], ".", label=f"{rate/1000:.0f} kHz/s")

    plt.legend()
    plt.xlabel("Time (UTC)")
    plt.ylabel("Frequency (MHz)")
    plt.title("Chirp Detections by Chirp Rate")
    plt.gcf().autofmt_xdate()
    plt.tight_layout()
    plt.show()


if __name__ == "__main__":
    main()

