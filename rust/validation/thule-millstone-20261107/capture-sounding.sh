#!/usr/bin/env bash
# Finite receiver-local IQ capture only. Wideband voltage is never transferred.
set -euo pipefail
config=${1:?config}; epoch=${2:?UTC sounding epoch}; binary=${3:?native verifier}
shift 3
if (($# == 0)); then set -- 5 7 9 11; fi
for mhz in "$@"; do
  # 25 kHz steps, 0.32 s dwell. Include both fine carriers and overlapping
  # windows spanning the entire dwell, with room for propagated late arrivals.
  read -r lo hi fire < <(awk -v f="$mhz" -v e="$epoch" 'BEGIN {t=(f-2)*12.8;printf "%.2f %.2f %.0f\n",t-.04,t+.28,e+t+12}')
  calendar=$(date -u -d "@$fire" '+%Y-%m-%d %H:%M:%S UTC')
  rf=$((mhz*1000000)); fine=$((rf+5000))
  timing=(--on-calendar="$calendar")
  now=$(date -u +%s)
  if (( fire < now )); then
    (( now-fire < 90 )) || { echo 'Capture already outside safe retained-data margin' >&2; exit 2; }
    timing=(--on-active=1s)
  fi
  systemd-run --user --unit="thule-millstone-${epoch}-${mhz}" \
    "${timing[@]}" --timer-property=AccuracySec=1s \
    --property=Nice=15 --property=CPUQuota=100% --property=MemoryMax=256M \
    --property=RuntimeMaxSec=90 \
    "$binary" "$config" --grid "$epoch" "$lo" "$hi" 0.04 "$rf" "$fine" 5000 --dense
done
