# Rust station processing

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
