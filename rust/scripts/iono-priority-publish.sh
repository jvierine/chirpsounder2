#!/usr/bin/env bash
# One capped upload worker: latest ionograms first, resumable bulk in short slots.
set -euo pipefail
station=${1:?station};output=${2:?output};destination=${3:-j@juha.no}
[[ "$station" =~ ^(DOB|KHO|TGO|W2NAF|KREA)$ ]] || exit 64
pending="${XDG_STATE_HOME:-$HOME/.local/state}/iono-publish/$station"
mkdir -p "$pending"
exec 9>"$pending/publish.lock"
flock -n 9 || exit 0
transfer() {
    timeout --signal=INT --kill-after=10 "$1" rsync -z --times --partial-dir=.rsync-partial --bwlimit=16 --timeout=90 --stats \
      -e 'ssh -oBatchMode=yes -oStrictHostKeyChecking=yes -oConnectTimeout=15 -oServerAliveInterval=15 -oServerAliveCountMax=4' \
      "$2" "$destination:${2##*/}"
}
last_latest=;last_bulk=
while true; do
    day=$(date -u +%F);epoch=$(date -u -d "$day" +%s)
    source="$output/$day/browser_latest-$station-$epoch.h5"
    if [[ -f "$source" ]]; then
      fingerprint=$(stat -c '%s:%Y:%i' "$source")
      if [[ "$fingerprint" != "$last_latest" ]]; then
        snapshot="$pending/${source##*/}"
        rm -f "$snapshot"
        ln "$source" "$snapshot" 2>/dev/null || cp "$source" "$snapshot"
        if transfer 120 "$snapshot"; then
          last_latest=$fingerprint;rm -f "$snapshot"
          echo "Priority ionograms published for $station" >&2
        fi
      fi
    fi
    source="$output/$day/browser_day-$station-$epoch.h5"
    snapshot="$pending/${source##*/}"
    if [[ -f "$source" ]]; then
      fingerprint=$(stat -c '%s:%Y:%i' "$source")
      if [[ -f "$snapshot" || "$fingerprint" != "$last_bulk" ]]; then
        if [[ ! -f "$snapshot" ]]; then ln "$source" "$snapshot" 2>/dev/null || cp "$source" "$snapshot"; fi
        # Allow useful progress through reconnects on constrained station links,
        # while returning to priority ionograms at least every two minutes.
        if transfer 120 "$snapshot"; then
          last_bulk=$(stat -c '%s:%Y:%i' "$snapshot");rm -f "$snapshot"
          echo "Bulk history published for $station" >&2
        fi
      fi
    fi
    sleep 5
done
