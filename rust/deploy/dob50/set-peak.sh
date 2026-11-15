#!/usr/bin/env bash
set -euo pipefail
[[ $(hostname -s) == tgo-mic1 ]] || exit 64
peak=${1:?Expected fractional 8-bit peak, e.g. 0.03125}
[[ $peak =~ ^[0-9]+([.][0-9]+)?$ ]] || exit 64
awk -v p="$peak" 'BEGIN{exit !(p>=1/256 && p<=1)}'
base=/home/tgo/src/dob50-trial
mkdir -p ~/.config/systemd/user/dob50-acquisition.service.d
cat > ~/.config/systemd/user/dob50-acquisition.service.d/zzzz-quantizer-scale.conf <<UNIT
[Service]
ExecStart=
ExecStart=$base/rx_uhd_ext_gps --mode 50mhz --outdir /dev/shm/hf50 --compat25-outdir /dev/shm/hf25 --stats-dir /data0/recorder-health/dob50 --peak $peak
UNIT
systemctl --user daemon-reload
systemctl --user restart dob50-acquisition
printf 'DOB sc8 peak fraction is now %s; inspect hourly rail-hit and overflow counters.\n' "$peak"
