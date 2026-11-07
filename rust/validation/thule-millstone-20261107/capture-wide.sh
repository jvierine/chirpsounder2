#!/usr/bin/env bash
# Finite wider packet-timing capture; IQ remains on the receiver.
set -euo pipefail
config=${1:?config}; epoch=${2:?epoch}; binary=${3:?verifier}
for mhz in 7 9 11; do
 read -r lo hi fire < <(awk -v f="$mhz" -v e="$epoch" 'BEGIN {t=(f-2)*12.8;printf "%.2f %.2f %.0f\n",t-1,t+1,e+t+14}')
 calendar=$(date -u -d "@$fire" '+%Y-%m-%d %H:%M:%S UTC')
 systemd-run --user --unit="thule-wide-${epoch}-${mhz}" --on-calendar="$calendar" --timer-property=AccuracySec=1s \
   --property=Nice=15 --property=CPUQuota=100% --property=MemoryMax=256M --property=RuntimeMaxSec=90 \
   "$binary" "$config" --grid "$epoch" "$lo" "$hi" 0.08 "$((mhz*1000000))" "$((mhz*1000000+5000))" 5000 --dense
done
