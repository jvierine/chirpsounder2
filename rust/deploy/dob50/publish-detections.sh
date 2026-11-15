#!/usr/bin/env bash
# Install the DOB-only full-band browser-array worker; acquisition is unchanged.
set -euo pipefail
[[ $(hostname -s) == tgo-mic1 ]] || exit 64
base=/home/tgo/src/dob50-trial
[[ -x "$base/iono-plots" ]] || { echo 'Install the matching iono-plots binary first'; exit 1; }
mkdir -p ~/.config/systemd/user/iono-browser-arrays.service.d
cat > ~/.config/systemd/user/iono-browser-arrays.service.d/zzzzzzz-dob50.conf <<UNIT
[Service]
Environment=IONO_SWEEP_MAX_MHZ=50
ExecStart=
ExecStart=$base/iono-plots %h/.config/iono-browser.json --browser-only --no-upload
UNIT
systemctl --user daemon-reload
systemctl --user restart iono-browser-arrays
