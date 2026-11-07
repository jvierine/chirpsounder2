#!/usr/bin/env bash
# Bounded 100 ksample/s commissioning caches at the actual fixed read origin.
set -euo pipefail
config=${1:?}; epoch=${2:?}; offset_us=${3:?}; binary=${4:?}; label=${5:?}
shift 5
if (($#==0));then set -- 5 7 9 11;fi
for mhz in "$@"; do
 read -r lo hi fire < <(awk -v f="$mhz" -v e="$epoch" -v o="$offset_us" 'BEGIN {s=(f-2)*12.8+o/1000000;printf "%.9f %.9f %.0f\n",s,s+.16,e+s+15}')
 calendar=$(date -u -d "@$fire" '+%Y-%m-%d %H:%M:%S UTC')
 timing=(--on-calendar="$calendar")
 now=$(date -u +%s)
 if ((fire<=now)); then ((now-fire<70)) || { echo 'Outside retained-voltage safety margin' >&2;exit 2; };timing=(--on-active=1s);fi
 systemd-run --user --unit="cc-origin-${label}-${epoch}-${mhz}" "${timing[@]}" --timer-property=AccuracySec=1s \
 --property=Nice=15 --property=CPUQuota=100% --property=MemoryMax=256M --property=RuntimeMaxSec=60 \
 "$binary" "$config" --grid "$epoch" "$lo" "$hi" .16 "$((mhz*1000000))" "$((mhz*1000000+5000))" 5000 --dense
done
