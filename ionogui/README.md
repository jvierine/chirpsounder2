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

## Code map

| File/module | Responsibility |
| --- | --- |
| `crates/iono-server/src/browser_server.rs` | Inbox ingestion, SQLite metadata/cache lifecycle and HTTP endpoints |
| `crates/iono-server/src/path_quality.rs` | Freshness and quality classification; does not re-decode observations |
| `web/app.js` | Dashboard state, progressive plot scheduling, selection and resource disposal |
| `web/arrays.js` | Bounded streaming Float32 downloads and byte-limited browser cache |
| `web/health.js` | Distinguishes telemetry age, publication delay and explicit processing losses |
| `web/groups.js`, `preferences.js`, `presentation.js` | Panel grouping/order, saved preferences and sizing controls |
| `web/observation-time.js` | UTC calendar/time selection and live “Now” behaviour |
| `web/map-links.js` | Great circles, hit testing, hover labels and path display conventions |
| `web/propagation-globe.js`, `globe-camera.js` | HEIMDALL globe adapter and matching screen/hit-test projection |
| `web/vendor/heimdall-globe.js` | Imported WebGL globe implementation; regenerate with the import script |
| `web/link-history-popup.js`, `ionogram-popup.js` | Selected-link history and ionogram popup lifetimes/navigation |
| `scripts/test_*.mjs` | Browser logic regression tests without requiring a live receiver |

`/api/day` supplies prepared plot metadata; `/api/data/ID` streams its numeric
payload. `/api/frequency-limits` supplies path assessments, `/api/health`
receiver telemetry, and `/api/archive-days` indexed available dates. The browser
draws with Rustplotlib/WASM; it does not decode raw voltage. Server ingestion
reads complete HDF5 bundles prepared by `../rust/crates/iono-plots`.

There are two separate SQLite indexes: the receiver's scientific/profile index
and this server's disposable metadata/transport catalogue. In the server,
`bundles` stores bulk publications, `latest_bundles` priority ionograms, `arrays`
tracks compressed cache files, and `sources` remembers exact archive paths.
Cache eviction removes rehydratable entries, not original scientific HDF5.
Prune before insertion as well as afterwards: a full catalogue otherwise blocks
the successful insertion that would have triggered cleanup.

Map colors come from assessed evidence and quality flags. A missing detection
does not establish transmitter silence; a generic source association is not a
verified transmitter site. Keep those distinctions when modifying map labels.
CPU use, data freshness and supported bands also remain separate signals.

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
