# Install the IonoGUI server

The server hosts the browser interface and indexes receiver-prepared HDF5
bundles. It does not record voltage, decode sounders, render ionogram images or
rebuild RTIs. A station installation is described in [INSTALL-STATION.md](INSTALL-STATION.md).

## 1. Install dependencies and clone

On Debian/Ubuntu:

```sh
sudo apt-get update
sudo apt-get install -y git curl ca-certificates build-essential pkg-config \
  libhdf5-dev nginx
mkdir -p ~/src
git clone --branch main --single-branch \
  https://github.com/jvierine/chirpsounder2.git ~/src/chirpsounder2
```

Install Rust through https://rustup.rs/ and load `. "$HOME/.cargo/env"`.
Install Node.js 26 or later for frontend tests and the globe import helper.
Build the Rustplotlib browser assets once; clients then render plots themselves.

```sh
cd ~/src/chirpsounder2/ionogui
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cargo build --release --locked --bin iono-browser-server
bash scripts/build_web.sh
node --test scripts/test_*.mjs
cargo test --locked --bin iono-browser-server
```

The script downloads the pinned public Rustplotlib source and installs the WASM
target through rustup. No receiver DSP service, OpenBLAS, Python plotting or TeX
installation is needed on this host. SQLite is bundled in the Rust server.

## 2. Install files and service account

```sh
sudo useradd --system --user-group --home-dir /var/lib/ionogui \
  --shell /usr/sbin/nologin ionogui
sudo install -d -o ionogui -g ionogui /var/lib/ionogui \
  /var/lib/ionogui/catalogue /var/lib/ionogui/inbox /srv/ionogui/archive
sudo install -d /opt/ionogui /opt/ionogui/web
sudo install -m755 target/release/iono-browser-server /opt/ionogui/
sudo cp -a web/. /opt/ionogui/web/
```

Run `useradd` only on the first installation. Keep the SQLite catalogue on local
disk. The scientific HDF5 archive may be a separate mounted disk/share; change
`IONO_ARCHIVE` and `RequiresMountsFor` below to match it. Keep enough local disk
for upload staging as well as the catalogue and compressed-array cache.

Create `/etc/systemd/system/ionogui.service`:

```ini
[Unit]
Description=Ionosonde prepared arrays and browser GUI
After=network-online.target
Wants=network-online.target
RequiresMountsFor=/srv/ionogui/archive

[Service]
User=ionogui
Group=ionogui
WorkingDirectory=/opt/ionogui
Environment=IONO_WEB=/opt/ionogui/web
Environment=IONO_STORE=/var/lib/ionogui/catalogue
Environment=IONO_INBOX=/var/lib/ionogui/inbox
Environment=IONO_ARCHIVE=/srv/ionogui/archive
Environment=IONO_LISTEN=127.0.0.1:8796
Environment=MALLOC_ARENA_MAX=2
ExecStart=/opt/ionogui/iono-browser-server
Restart=always
RestartSec=5
MemoryMax=384M
NoNewPrivileges=yes
PrivateTmp=yes

[Install]
WantedBy=multi-user.target
```

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now ionogui.service
curl --fail http://127.0.0.1:8796/api/health
```

An empty station list is expected until the first bundle is imported.

## 3. Import the first receiver bundle

Copy a receiver-created `browser_day-STATION-EPOCH.h5` into a dated directory
under `/srv/ionogui/archive/`, preserving its filename. The service account needs
read/write access to archive directories used for incoming bundles, and read
access to the files. For example, after transferring a real bundle to `/tmp`:

```sh
# Substitute the actual UTC date and exact received filename.
day=YYYY-MM-DD
bundle=browser_day-MYRX-EPOCH.h5
sudo install -d -o ionogui -g ionogui "/srv/ionogui/archive/$day"
sudo install -o ionogui -g ionogui -m640 "/tmp/$bundle" \
  "/srv/ionogui/archive/$day/$bundle"
sudo -u ionogui env IONO_STORE=/var/lib/ionogui/catalogue \
  IONO_INBOX=/var/lib/ionogui/inbox IONO_ARCHIVE=/srv/ionogui/archive \
  /opt/ionogui/iono-browser-server import "/srv/ionogui/archive/$day/$bundle"
curl --fail http://127.0.0.1:8796/api/archive-days
```

The import command indexes one exact bundle; it does not scan the observation
archive. For routine publication, your upload handler should atomically rename
each completed bundle into place and then atomically publish a JSON ticket in
`IONO_INBOX`, for example `MYRX-day.json` containing:

```json
{"path":"/srv/ionogui/archive/YYYY-MM-DD/browser_day-MYRX-EPOCH.h5"}
```

The long-running service polls the inbox every five seconds. Use separate ticket
names for bulk `browser_day` and priority `browser_latest` bundles; replace an
outstanding ticket atomically to coalesce updates. Never publish a ticket for a
partially written HDF5 file. The advanced local `staged_path` transport is
optional; its file must be inside `IONO_STORE/incoming`.

Upload authentication and SSH account setup are deliberately site-specific.
Use a restricted account/handler, per-station write permissions and rate limits.
The read-only HTTP service has no upload route. Do not expose the inbox or
archive as writable web directories, and do not commit passwords or keys.

## 4. Publish behind the web server

Create an nginx site using your real hostname. This example serves IonoGUI at
`/iono/`, preserving the frontend's relative API paths:

```nginx
server {
    listen 80;
    server_name iono.example.org;
    location = /iono { return 302 /iono/; }
    location /iono/ {
        proxy_pass http://127.0.0.1:8796/;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 60s;
    }
}
```

Enable that site, check `sudo nginx -t`, then reload nginx. Configure HTTPS with
your site's certificate tooling before public use; the example hostname is a
placeholder. Leave the Rust service on localhost. `/iono2/` may be redirected to
`/iono/` while preserving query parameters for existing bookmarks.

Open `/iono/?view=propagation` and a receiver view. Check real plots, link clicks,
UTC selection, zoom, and current product timestamps—not just an HTTP 200.
For a new receiver, add its public coordinates and display/gate configuration to
`ionogui/web/paths.json` and `ionogui/web/transmitters.json`; `paths.json` and
`transmitters.json` are embedded in the Rust binary, so rebuild and reinstall
the backend after changing them.

## 5. Maintenance and upgrades

```sh
systemctl status ionogui --no-pager
journalctl -u ionogui -n 50 --no-pager
df -h /var/lib/ionogui /srv/ionogui/archive
curl --fail http://127.0.0.1:8796/api/health
```

Catalogue metadata has byte-based eviction before imports and at startup;
each receiver's newest bulk/priority publication is preserved. The 512 MiB
SQLite page ceiling provides headroom above the two 32 MiB metadata budgets.
Compressed display arrays have a separate 96 MiB cache. Old days are reloaded
from indexed HDF5 bundles, and scientific observations are never cache-evicted.
Monitor both disk space and publication ages; a running service alone does not
prove fresh data.

Before upgrades, save the current binary/frontend and make a consistent SQLite
backup with SQLite's `.backup` command (do not copy only the live database while
its WAL is active). Pull a reviewed revision, build and test it, install the new
binary through a temporary filename and atomic rename, copy matching browser
assets, then restart only `ionogui.service`. Verify fresh imports and the rendered
page. Receiver acquisition and DSP need not restart for a GUI upgrade.
