#!/usr/bin/env bash
set -euo pipefail
[[ $(id -un) == administrator ]]
root=$(cd "$(dirname "$0")/../.." && pwd)
here="$root/deploy/kho-next"
for binary in iono-receiver iono-prn iono-verso iono-delay iono-plots rx_uhd_ext_gps iono-usrp-status; do
    test -x "$HOME/bin/$binary"
    if LD_LIBRARY_PATH="$HOME/.local/lib" ldd "$HOME/bin/$binary" | grep -q 'not found'; then exit 1; fi
done
program="$HOME/.config/chirpsounder2/verso/SGO_VERSO.h5"
printf '%s  %s\n' 019295c4569f9cce10e39a2c34961d8f67a612372f2f29f3d63e58dd5eee5220 "$program" | sha256sum -c -
mkdir -p "$HOME/.config/systemd/user" /data0 "$HOME/.local/state/kho-install"
cp -a "$here/config/." "$HOME/.config/"
for name in wait-ring wait-usrp ring-clean publish-latest; do
    install -m755 "$here/$name.sh" "$HOME/bin/kho-$name.sh"
done
install -m644 "$here"/*.service "$here/iono-kho.target" "$HOME/.config/systemd/user/"
install -m644 "$root/deploy/nordic/"*.service "$root/deploy/iono-prn@.service" \
    "$root/deploy/iono-prn.slice" "$root/deploy/iono-verso.service" "$root/deploy/iono-digisonde@.service" "$HOME/.config/systemd/user/"
cat > "$HOME/.config/iono-browser.json" <<'EOF'
{"station":"KHO","data_dir":"/dev/shm/hf25","output_dir":"/data0","browser_interval_s":300,"browser_upload_kib_s":128,"browser_previous_day":false}
EOF
cat > "$HOME/.config/systemd/user/iono-browser-arrays.service" <<'EOF'
[Unit]
Description=KHO receiver-local scientific browser products
[Service]
WorkingDirectory=%h/src/chirpsounder2/rust
ExecStart=%h/bin/iono-plots %h/.config/iono-browser.json --browser-only
Restart=always
RestartSec=15
Nice=19
IOSchedulingClass=idle
MemoryMax=768M
LimitAS=1610612736
EOF
workers=(iono-rust-detect iono-rust-digisonde iono-rust-lfm iono-rust-spectrum
         iono-prn@Kiruna iono-prn@Uppsala iono-prn@Montelibretti
         iono-verso iono-digisonde@thule iono-browser-arrays)
for unit in "${workers[@]}"; do
    mkdir -p "$HOME/.config/systemd/user/$unit.service.d"
    cat > "$HOME/.config/systemd/user/$unit.service.d/kho-handover.conf" <<'EOF'
[Unit]
PartOf=iono-kho.target
After=iono-kho-acquisition.service
[Service]
ExecStartPre=%h/bin/kho-wait-ring.sh
TimeoutStartSec=infinity
EOF
done
mkdir -p "$HOME/.config/systemd/user/iono-prn@.service.d"
cat > "$HOME/.config/systemd/user/iono-prn@.service.d/openblas.conf" <<'EOF'
[Service]
Environment=IONO_PRN_LAPACK=/usr/lib/x86_64-linux-gnu/openblas-pthread/libopenblas.so.0
Environment=OPENBLAS_NUM_THREADS=1
Environment=OMP_NUM_THREADS=1
EOF
cat > "$HOME/.config/systemd/user/iono-rust-detect.service.d/background.conf" <<'EOF'
[Service]
Nice=19
CPUWeight=1
MemoryMax=2G
EOF
systemctl --user daemon-reload
systemd-analyze --user verify "$HOME/.config/systemd/user/iono-kho.target"
git -C "$root/.." rev-parse HEAD > "$HOME/.local/state/kho-install/source-revision"
# The root carrier watcher starts/stops this target. Do not enable individual
# acquisition, DSP or publisher units independently of the physical link.
