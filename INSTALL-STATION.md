# Install a receiver station

Use a Linux receiver with a GPS-timed USRP and a local DigitalRF voltage ring.
The recorder owns the USRP; Rust processors read the ring without retuning it.
The station prepares sparse HDF5 ionograms, spectra and RTIs. The web server
only indexes and serves those products.

These commands install software as your normal station account on Debian or
Ubuntu. They do not overwrite an existing station configuration or start all
available sounders. Start with one sounder, verify it, then add others.

## 1. Get the software and dependencies

```sh
sudo apt-get update
sudo apt-get install -y git curl ca-certificates build-essential pkg-config \
  libhdf5-dev libopenblas-dev rsync
mkdir -p ~/src ~/bin ~/.config/chirpsounder2 ~/.config/systemd/user
git clone --branch main --single-branch \
  https://github.com/jvierine/chirpsounder2.git ~/src/chirpsounder2
```

If Rust is not installed, install rustup from https://rustup.rs/ and load its
environment with `. "$HOME/.cargo/env"`. An existing checkout should be updated
with `git pull --ff-only` after reviewing local changes, rather than cloned over.

```sh
cd ~/src/chirpsounder2/rust
cargo build --release --locked -p iono-receiver -p iono-plots
install -m755 target/release/iono-receiver target/release/iono-prn \
  target/release/iono-verso target/release/iono-delay \
  target/release/iono-plots ~/bin/
```

Build on the receiver, or a machine with compatible Linux/glibc and HDF5.
SIMD dispatch selects supported AVX/FMA, AVX or SSE kernels at runtime.
Do not distribute a `target-cpu=native` build to different machines.
No Python plotting environment or TeX is needed for browser arrays.
Optional static Rustplotlib figures use a normal TeX installation.

## 2. Set up local storage and acquisition

```sh
sudo mkdir -p /data0
sudo chown "$(id -un):$(id -gn)" /data0
test -r /dev/shm/hf25/ch0/drf_properties.h5
```

Use the last command only after acquisition is running; change the path if your
ring is elsewhere. `/data0` must be persistent storage, not the voltage RAM ring.
Give a new receiver a unique acronym and record its antenna coordinates.

If the USRP recorder is already working, leave it running. On a fresh machine,
install `uhd-host`, `libuhd-dev`, the Boost development packages listed in
[`rust/deploy/krea/install-dependencies.sh`](rust/deploy/krea/install-dependencies.sh),
and the DigitalRF C library and pkg-config files. The root `make` builds the C++
UHD recorders; all subsequent signal processing is Rust. Check:

```sh
uhd_find_devices
uhd_usrp_probe
pkg-config --modversion uhd hdf5 digital_rf
cd ~/src/chirpsounder2
make rx_uhd_ext_gps
./rx_uhd_ext_gps --help
```

Set the USRP Ethernet interface, GPS/10 MHz/PPS references and acquisition service
for the actual hardware before starting the recorder. The KREA acquisition and
ring-cleaner units in `rust/deploy/krea/` are examples, not a generic installer:
their IP address, account, memory limits and twenty-second retention are specific
to KREA. Size RAM retention to cover the longest processing lookback and writer
latency. Never run a second recorder against the same USRP.

## 3. Configure one sounder

For a PRN example, copy an existing transmitter program:

```sh
mkdir -p ~/.config/iono-prn
cp ~/src/chirpsounder2/rust/deploy/prn/DOB-Kiruna.json \
  ~/.config/iono-prn/Kiruna.json
```

Edit `station`, `data_dir`, `output_dir`, channel and propagation-range interval
for your receiver. Preserve the transmitter's actual code, schedule and carrier
program. Do not copy another receiver's hardware timing correction. Use the
[`iono-delay` diagnostics](rust/README.md) to choose a plausible one-way range
window; a geometric estimate is not a receiver clock calibration.

```sh
export OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1
~/bin/iono-prn ~/.config/iono-prn/Kiruna.json --once
```

The sweep must still be available in the ring. Inspect the output and logs before
enabling continuous reception. Empty sparse output is a non-detection, not proof
that the transmitter is off. See [`rust/deploy/prn/README.md`](rust/deploy/prn/README.md)
for PRN programs; `rust/deploy/stations/`, `digisonde-candidates/` and `verso/`
contain the other families' configurations.

## 4. Start automatically after reboot

```sh
cd ~/src/chirpsounder2
install -m644 rust/deploy/iono-prn@.service rust/deploy/iono-prn.slice \
  ~/.config/systemd/user/
bash rust/deploy/openblas/install.sh
sudo loginctl enable-linger "$(id -un)"
systemctl --user daemon-reload
systemctl --user enable --now iono-prn@Kiruna.service
journalctl --user -u iono-prn@Kiruna.service -n 30 --no-pager
```

The OpenBLAS helper installs a station-local library and configures PRN workers
to use one BLAS thread. Linger starts user services without an interactive login.
Use restartable acquisition units too, so USRP power cycling recovers on its own.
Set BIOS **AC Recovery / Power On** separately if the PC must boot after mains
failure. Service restart does not replace that BIOS setting.

## 5. Prepare browser arrays locally

Create `~/.config/iono-browser.json`, replacing `MYRX` and paths:

```json
{
  "station": "MYRX",
  "data_dir": "/dev/shm/hf25",
  "output_dir": "/data0",
  "browser_interval_s": 300,
  "browser_upload_kib_s": 32,
  "browser_previous_day": false
}
```

For a local check:

```sh
~/bin/iono-plots ~/.config/iono-browser.json --browser-only --no-upload
```

This is a continuous worker; stop it before starting its service. Copy
`rust/deploy/iono-browser-arrays.service` into `~/.config/systemd/user/`, append
`--no-upload` to `ExecStart` while arranging your own transport, then enable it:

```sh
systemctl --user daemon-reload
systemctl --user enable --now iono-browser-arrays.service
```

It writes `browser_day-MYRX-EPOCH.h5` and priority `browser_latest` bundles beside
the scientific products in dated `/data0` directories. Send these prepared
bundles to your server using the [server guide](INSTALL-SERVER.md). Do not send
raw voltage or let the web server rebuild RTIs from individual observations.
Keep upload keys outside Git and use a restricted upload account. Establish a
bandwidth budget before enabling publication, especially on constrained links.

## Check and update

```sh
systemctl --user --failed
journalctl --user -u iono-browser-arrays.service -n 30 --no-pager
df -h /data0 /dev/shm
```

Confirm both acquisition timestamps and prepared-product timestamps advance.
Add high-priority scheduled sounders first. Global chirp discovery is background
work: plan one core's worth of capacity, with additional spare cycles available,
without starving scheduled reception. Do not enable every example service.

For upgrades, build a reviewed revision first and use
`rust/scripts/install-station-build.sh BUILD_DIR SOURCE_ROOT` for an existing
Rust deployment. It preserves configuration and acquisition and saves rollback
copies. See [`rust/README.md`](rust/README.md) for details.
