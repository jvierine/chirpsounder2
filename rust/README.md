# Rust station processing

Supported sounder families: **Digisonde**, **LFM/chirp**, and **PRN/coded**.

This workspace belongs to **chirpsounder2**. It runs beside the receiver's
DigitalRF voltage ring buffer. IonoGUI2 is a separate browser GUI and thin
archive service; a station does not need the IonoGUI2 repository to build or
run these programs.

For an existing receiver, `scripts/install-station-build.sh BUILD_DIR SOURCE_ROOT`
installs a reviewed native build under `~/bin` and updates only running Rust
worker command lines to that public source release. It preserves JSON contents,
resource limits, upload caps and acquisition, and saves the previous binaries
and units under `~/.local/state/chirpsounder2-rollout`. Old GUI-owned config
locations are copied to `~/.config/chirpsounder2/`. Review the script and native
library compatibility before running it; do not run a build for another glibc.

| Program | Station task |
| --- | --- |
| `iono-receiver detect` | Detect HF chirps in the wideband recording |
| `iono-receiver lfm` | Receive chirp sounders |
| `iono-receiver digisonde` | Receive scheduled Digisonde transmissions |
| `iono-receiver spectrum` | RF spectrum/background products |
| `iono-digisonde-search` | Bounded PINS-style pulse discovery from the local ring |
| `iono-digisonde-verify` | Complementary-code, carrier and pulse-period checks |
| `iono-prn` | Receive scheduled pseudorandom coded sounders |
| `iono-plots` | Incremental SQLite indexing, RTI reduction, bounded HDF5 browser arrays and optional Rustplotlib figures |
| `iono-render` | Legacy Rustplotlib static-figure compatibility |

`iono-core` contains DSP and HDF5 product readers. `iono-browser` defines the
prepared HDF5 display-product contract shared with the GUI service. The GUI
consumes these shared types with `iono-core` default features disabled, so
receiver DSP modules and RustFFT are not compiled into the web service.

## Build on a Linux receiver

Install Rust with the official rustup installer, then install HDF5 development
headers. The SQLite dependency is bundled.

```sh
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libhdf5-dev
git clone https://github.com/jvierine/chirpsounder2.git
cd chirpsounder2/rust
export HDF5_DIR=/usr/lib/x86_64-linux-gnu/hdf5/serial
cargo build --release --locked -p iono-receiver -p iono-plots
cargo test --workspace --locked
mkdir -p ~/bin
install -m755 target/release/iono-receiver target/release/iono-prn target/release/iono-plots ~/bin/
```

Build against the target station's glibc/HDF5. Ubuntu 24.04 binaries are not
portable to Ubuntu 20.04. Do not use `-C target-cpu=native` when distributing
binaries to other receivers. Optional static Rustplotlib figures need TeX
and its normal native dependencies; `iono-plots --browser-only` produces
numeric arrays without invoking TeX or generating images.

## PRN reception

See [the reviewed transmitter configurations](deploy/prn/README.md).
The worker reads the existing **25 MHz complex-voltage DigitalRF ring buffer**
at `/dev/shm/hf25/ch0`; it does not open UHD, retune the USRP or replace
acquisition. Change station/output paths in a copied configuration as needed.

```sh
mkdir -p ~/.config/iono-prn
cp deploy/prn/DOB-Kiruna.json ~/.config/iono-prn/Kiruna.json
# Edit the copied JSON for your receiver name and paths.
~/bin/iono-prn ~/.config/iono-prn/Kiruna.json
```

The station and transmitter identifiers use letters, digits and underscores.
`--once` receives one sweep; `--start UNIX_SECONDS` selects a retained epoch
for a replay (the corresponding voltage must still exist).

Install `deploy/iono-prn@.service` and `deploy/iono-prn.slice` under
`~/.config/systemd/user/`, then:

```sh
systemctl --user daemon-reload
systemctl --user enable --now iono-prn@Kiruna
journalctl --user -u iono-prn@Kiruna -f
```

The units use `~/bin/iono-prn` and `~/.config/iono-prn/NAME.json`. The shared
slice bounds the total worker CPU/memory. Existing recording continues
independently, including when a PRN worker restarts.

## Digisonde discovery and W2NAF

The discovery tools port the 2019 PINS pulse-folding and complementary-code
search. They read the existing ring and write HDF5; they never retune UHD.
`iono-digisonde-search CONFIG --lookback 900` observes 160 ms every 320 ms,
uses four bounded workers, and reports coarse pulse candidates after spectral
soft clipping. Config keys are `station`, `data_dir`, `output_dir`, `channel`,
optional `frequency_range_hz` (default `[700000,24500000]`) and
`max_candidates` (default 6, maximum 64 per window).

`iono-digisonde-verify CONFIG SEARCH_HDF5 128` checks candidates against
5/10 ms complementary-code modes. A commissioning grid can instead be read
with `CONFIG --grid UNIX_EPOCH TMIN TMAX DT FMIN FMAX DF`, in seconds and Hz.
Carrier checks cover ±100 Hz by default (`doppler_search_hz` in the config).
Recurrence, matched-filter
score and code-mode aliases alone do not identify a transmitter; require a
consistent stepped-frequency track and independent schedule evidence.
Lag is a circular correlation coordinate, not a calibrated propagation range.

`deploy/w2naf-millstone.json` receives the Millstone Hill MHJ45 schedule:
450 s cadence, 2 MHz starting frequency, 25 kHz steps and 32 × 10 ms IPPs
per step. The cadence was checked against current GIRO records; consecutive
steps and a consistent coded echo were measured in W2NAF's voltage on
2026-11-06. A continuous return around 3–6 MHz was decoded and rendered in
the live GUI. The configured upper sweep limit is a reception search extent,
not a measured usable-frequency limit. Transmitter group delay remains
uncalibrated and is explicitly recorded in `range_reference`.

For this receiver, optional `rfi_clip_ratio` suppresses narrow spectral peaks
relative to the in-passband median while preserving phase. Real voltage
overlap prevents artificial FIR edges; no samples are invented over gaps.
`snr_threshold_db` controls sparse acceptance. Both options preserve historical
behavior when absent. Install the JSON under `~/.config/chirpsounder2/` and
`deploy/iono-digisonde@.service` under `~/.config/systemd/user/`, then:

```sh
systemctl --user daemon-reload
systemctl --user enable --now iono-digisonde@w2naf-millstone
journalctl --user -u iono-digisonde@w2naf-millstone -f
```

Other nearby transmitters require independent discovery and commissioning;
no Wallops or Alpena identity is assigned to unverified pulse candidates.

## Acceleration and output

Complex64 means two f32 components. The kernels select AVX+FMA, AVX, SSE3,
or scalar at runtime; SSE4-capable CPUs are supported by the SSE fallback.
RustFFT independently selects compatible FFT kernels. KHO's Ivy Bridge
processor uses AVX without requiring AVX2 or FMA. Startup logs report the
selected implementation. For a bounded per-machine comparison:

```sh
cargo test -p iono-core --release benchmark_receiver_kernels -- --ignored --nocapture
```

PRN HDF5 files retain `freqs`, `ranges`, completion/timing/code-identity
metadata, and frequency-major `SNR_sparse_indices`/`SNR_sparse_values`.
Only accepted cells are stored; absent cells mean unmeasured/rejected data.
Routine defaults are an 8 dB display threshold, a 3 dB target/control code
margin and a 0–20 dB display scale. Automatic echo-band estimates require
13 dB. Dense commissioning diagnostics are optional and normally disabled.

Matched filtering retains code sidelobes; the code margin is a commissioning
guard, not a calibrated false-alarm probability. Transmitter/receiver group
delay and operational usable-frequency limits require physical validation.

Receiver-local `iono-plots --browser-only` prepares the RTIs and display
arrays from local HDF5. The web host serves those arrays. The existing
bandwidth-capped delta publisher can upload the prepared products without
shipping raw voltage or making the web host scan individual observations.
