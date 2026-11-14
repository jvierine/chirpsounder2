#!/usr/bin/env bash
# Install overlays on DOB without editing its old checkout or existing binaries.
set -euo pipefail
[[ $(hostname -s) == tgo-mic1 ]] || exit 64
base=/home/tgo/src/dob50-trial
mkdir -p "$base/install-state" ~/.config/systemd/user
for unit in iono-prn@Montelibretti iono-rust-detect; do
    [[ ! -e "$HOME/.config/systemd/user/$unit.service.d/zz-dob50.conf" ]] || { echo 'Existing DOB50 overlay: inspect before reinstalling'; exit 1; }
done
systemctl --user list-units 'iono-*' --type=service --state=active --no-legend --plain | awk '{print $1}' > "$base/install-state/previously-active.txt"
systemctl --user cat chirpsounder_dombas > "$base/install-state/old-acquisition-unit.txt"
systemctl --user is-enabled chirpsounder_dombas > "$base/install-state/old-enabled.txt" || true
cp ~/.config/iono-prn/Montelibretti.json "$base/install-state/Montelibretti-original.json"
~/ionosonde/bin/python "$base/prepare-config.py"
for binary in iono-prn iono-receiver; do
    gzip -dc "$base/binaries/dob50-$binary.gz" > "$base/$binary"
    chmod 755 "$base/$binary"
done
cat > ~/.config/systemd/user/dob50-acquisition.service <<UNIT
[Unit]
Description=DOB experimental 0–50 MHz, signed 8-bit UHD acquisition
Conflicts=chirpsounder_dombas.service
Wants=dob50-clean.service
After=network-online.target dob50-clean.service
StartLimitIntervalSec=0
[Service]
ExecStartPre=/bin/bash -c 'rm -rf /dev/shm/hf50/ch0 /dev/shm/hf25/ch0'
ExecStart=$base/rx_uhd_ext_gps --mode 50mhz --outdir /dev/shm/hf50 --compat25-outdir /dev/shm/hf25 --stats-dir /data0/recorder-health/dob50 --peak 0.125
Restart=always
RestartSec=5
KillSignal=SIGINT
TimeoutStopSec=20
Nice=0
CPUWeight=2000
LimitCORE=0
[Install]
WantedBy=default.target
UNIT
cat > ~/.config/systemd/user/dob50-clean.service <<UNIT
[Unit]
Description=DOB bounded raw-voltage RAM rings (50 MHz 40 s; 25 MHz 90 s)
[Service]
ExecStart=/bin/bash $base/clean-ring.sh
Restart=always
RestartSec=2
[Install]
WantedBy=default.target
UNIT
mkdir -p ~/.config/systemd/user/iono-prn@Montelibretti.service.d ~/.config/systemd/user/iono-rust-detect.service.d
cat > ~/.config/systemd/user/iono-prn@Montelibretti.service.d/zz-dob50.conf <<UNIT
[Service]
ExecStart=
ExecStart=$base/iono-prn %h/.config/chirpsounder2/Montelibretti50.json
MemoryMax=2G
UNIT
cat > ~/.config/systemd/user/iono-rust-detect.service.d/zz-dob50.conf <<UNIT
[Service]
ExecStart=
ExecStart=$base/iono-receiver detect %h/.config/chirpsounder2/receiver50.json
Nice=19
CPUWeight=1
UNIT
cat > ~/.config/systemd/user/dob50-spectrum.service <<UNIT
[Unit]
Description=DOB local-only experimental full 0–50 MHz spectrum
After=dob50-acquisition.service
[Service]
ExecStart=$base/iono-receiver spectrum %h/.config/chirpsounder2/spectrum50.json
Restart=always
RestartSec=10
Nice=19
CPUWeight=1
MemoryMax=1G
[Install]
WantedBy=default.target
UNIT
systemctl --user daemon-reload
printf 'Installed DOB50 overlays; acquisition is not switched yet.\n'
