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
| `iono-verso` | Receive the Finnish SGO VERSO coded/TDM network from its supplied waveform and timing program |
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
install -m755 target/release/iono-receiver target/release/iono-prn target/release/iono-verso target/release/iono-plots ~/bin/
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

## Finnish VERSO reception

See [the waveform, timing interpretation and receiver configurations](deploy/verso/README.md).
`iono-verso` imports the supplied YAML and complex128 baud file into an HDF5
program, then performs receiver-local complex64 pulse compression and coherent
integration with the 10 ms inter-channel delays removed. It writes sparse PRN
products compatible with the existing receiver-side RTI/browser preparation.
The GUI host never processes its voltage. The station installer also updates
an active VERSO unit when the reviewed build contains `iono-verso`.

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

## Station capacity and receiver priority

`deploy/stations/` records the deployed native DSP JSON configurations and
checksums. Acquisition keeps its existing priority. Ramfjordmoen Digisonde
processing is first, scheduled SGO LFM second, and global chirp detection third;
additional sounders run below these. `scripts/station-priority.sh --install`
applies cgroup CPU weights and process niceness without restarting acquisition
or changing receiver timings, existing hard CPU quotas or memory limits. Its
`iono-priority.timer` retains the ordering when legacy children respawn.

Digisonde reception reuses the runtime-dispatched AVX/FMA, AVX or SSE mixer/FIR
for each frequency dwell. Its 30 kHz FIR bandwidth and phase convention match
the historical scalar reference. The complementary-code finder reuses FFT
scratch storage and computes the reported median only for the winning match;
it retains the full carrier, Doppler and code-mode search.

W2NAF's `iono-digisonde-watch.timer` repeats a bounded receiver-local PINS scan
and code verification every three minutes. `w2naf-digisonde-watch.json` reserves
candidates across MHz bands, and verification also distributes its budget
across bands. This prevents strong distant radar candidates from monopolizing
the nearby-Digisonde search. HDF5 diagnostics remain on the receiver, not on
juha.no. A code-match row is a diagnostic candidate, not a station identity.
New receivers need consistent stepped-frequency tracks and independent timing
confirmation; do not guess US station names from isolated correlation peaks.

### Limited-bandwidth publication

All native sounder products store accepted cells as sparse HDF5 index/value
arrays, including both Digisonde modes. This changes storage, not detection
thresholds. RF background spectra remain full average-power products.
Browser transport retains one chunked numeric HDF5 representation, without
a duplicate HTTP encoding. The thin host caches standard gzip serialization
of these already computed cells; it performs no scientific reduction or image
rendering. Stable UTC-aligned time bins permit efficient delta transfers while
the browser displays the requested sliding 48-hour interval. Original
scientific files retain the measured precision.

`browser_latest` bundles contain only current ionograms and their reduced
frequency evidence. They update independently of full-day RTIs and 48-hour
history. The thin host overlays them by transmitter/product and observation
time, so a slower bulk arrival cannot erase a newer sounding. DOB's
`scripts/iono-priority-publish.sh` sends these small bundles first, then gives
resumable bulk history a 45-second slot. One worker shares the existing
16 KiB/s cap; it never queues old-day history ahead of current observations.

PRN receiver configs may set `writer_grace_s` (0.25–6 seconds). This delays
reading until the recorder's files are available; it does not change the
transmitter schedule or propagation calibration. `iono-prn CONFIG --ring-status`
checks recent 100 ms sample windows, then rechecks the same windows with a
fresh reader three seconds later. At KHO, windows 1–3 seconds old were initially
unavailable and subsequently present. Its configured grace is therefore 4.5 s.
Background RF plotting, monitoring and housekeeping have lower scheduling
priority than acquisition and the three primary receiver processing tasks.

## Native RF monitoring

KHO's RF worker is `iono-receiver spectrum`, installed as the enabled user unit
`iono-rust-spectrum.service` from `deploy/kho/`. Its configuration is recorded
in `deploy/stations/KHO/spectrum.json`. The KHO startup script starts this unit
instead of `plot_rf_spec.py`; no Python RF plotting supervisor is required.
The existing receiver-local array publisher prepares the HDF5 products for
browser rendering, without server-side image generation.

The worker averages positive linear FFT power and converts to dB without
background subtraction. The default integrates all non-overlapping 8192-point
Hann windows. `spectrum_windows_per_bin` optionally bounds the FFT count for
each half-second bin. KHO uses 128 windows in stratified time blocks with
changing deterministic offsets, covering all frequency bins while reducing
ring-buffer reads and computation. This samples the half-second interval,
rather than integrating every voltage sample; short transient occupancy can
therefore vary. HDF5 metadata records the available and averaged window counts
and sampling strategy. Power normalization uses the actual averaged count.
`writer_grace_s` allows the DigitalRF recorder to finish its current file before
reading; KHO uses 4.5 seconds.

The KHO unit runs below acquisition and sounder reception priority, with a
75%-of-one-core CPU ceiling and a 192 MiB memory ceiling. It restarts on failure
and starts with the user manager after reboot. Keep the station's existing
user lingering enabled. No acquisition restart is needed to replace the RF
worker.
