#!/usr/bin/env bash
set -euo pipefail
station=${1:?station}
epoch=${2:?common UTC epoch}
helper=${3:?native verifier path}
config_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
[[ -x "$helper" ]]
[[ "$epoch" =~ ^[0-9]{10}$ ]]
case "$station" in DOB|KHO|TGO|W2NAF) ;; *) echo "unsupported receiver" >&2; exit 2;; esac
[[ $(date +%s) -lt $((epoch+103)) ]]
for task in phase control; do
 delay=103; tmin=89.28; tmax=90.24
 if [[ "$task" == control ]]; then delay=116; tmin=102.08; tmax=103.04; fi
 utc=$(date -u -d "@$((epoch+delay))" '+%Y-%m-%d %H:%M:%S UTC')
 systemd-run --user --unit="iono-${task}-${station}-${epoch}" --on-calendar="$utc" --timer-property=AccuracySec=100ms --property=Nice=19 --property=CPUQuota=50% --property=MemoryMax=256M --property=RuntimeMaxSec=120 "$helper" "${config_dir}/${task}.json" --grid "$epoch" "$tmin" "$tmax" .16 8990000 9020000 5000
done
