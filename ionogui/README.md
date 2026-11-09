# IonoGUI

Start with the [server installation guide](../INSTALL-SERVER.md), or the
[receiver station guide](../INSTALL-STATION.md) for acquisition and processing.

Public browser interface and thin Rust data service for receiver-prepared
Digisonde, LFM/chirp and PRN ionosonde products. The live interface is at
https://juha.no/iono/.

The `web/` directory contains the interactive Rustplotlib/WASM plots and
HEIMDALL-based propagation globe. `crates/iono-server` indexes receiver-prepared
HDF5 bundles and serves their compressed arrays. Reception, signal processing,
ionogram preparation and RTI preparation belong to the adjacent `../rust`
workspace and run at receivers, not in this web service.

## Build

Install Rust, the HDF5 development libraries (`libhdf5-dev` on Debian/Ubuntu),
Node.js 26 or later, and wasm-bindgen-cli 0.2.129. From this directory:

```sh
cargo build --release --locked --bin iono-browser-server
bash scripts/build_web.sh
node --test scripts/test_*.mjs
cargo test --locked --bin iono-browser-server
```

`build_web.sh` fetches the public Rustplotlib revision pinned in `web/app.js`
and builds its browser assets. Generated binaries and assets are not committed.

## Run

Configure these environment variables for your own installation:

```sh
export IONO_WEB="$PWD/web"
export IONO_STORE="$PWD/runtime/catalogue"
export IONO_INBOX="$PWD/runtime/inbox"
export IONO_ARCHIVE="$PWD/runtime/archive"
export IONO_LISTEN=127.0.0.1:8796
mkdir -p "$IONO_STORE" "$IONO_INBOX" "$IONO_ARCHIVE"
./target/release/iono-browser-server
```

Supply receiver-prepared `browser_day` HDF5 bundles through the catalogue's
inbox workflow. Scientific observations remain in the archive. Catalogue
metadata is evicted by byte budget while preserving each receiver's newest
publication; old days can be reloaded from their indexed archive bundles.
The SQLite page limit is 512 MiB, with separate 32 MiB metadata budgets for
bulk and priority publications and a 96 MiB array cache.

This directory contains public application code, tests, public station/map
configuration, fonts and branding assets. Production upload handlers, SSH
configuration, credentials, local data, logs and host deployment scripts are
not included. Configure authentication and uploads separately for your site.

The original GUI source snapshot is ionogui2 revision
`d49e039`; HEIMDALL globe reuse is attributed in its source comments.
