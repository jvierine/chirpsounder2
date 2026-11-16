#!/usr/bin/env bash
# Let DOB's single-threaded PRN workers use spare cores; retain CPU weights.
set -euo pipefail
[[ $(hostname -s) == tgo-mic1 ]] || exit 64
dir="$HOME/.config/systemd/user/iono-prn.slice.d"
mkdir -p "$dir"
printf '[Slice]\nCPUQuota=\n' > "$dir/zzzz-dob-throughput.conf"
systemctl --user daemon-reload
systemctl --user set-property --runtime iono-prn.slice CPUQuota=
systemctl --user show iono-prn.slice -p CPUQuotaPerSecUSec -p CPUWeight
