#!/usr/bin/env bash
# TGO's bulk HTTP uploader does not send its prepared browser_latest bundles.
# Publish only that small lane. Do not enable alongside a priority rsync worker.
set -euo pipefail
station=${1:?station}; output=${2:?output directory}
[[ "$station" == TGO ]] || exit 64
state="${XDG_STATE_HOME:-$HOME/.local/state}/iono-latest-http/$station"
mkdir -p "$state"
exec 9>"$state/publish.lock"
flock -n 9 || exit 0
day=$(date -u +%F); epoch=$(date -u -d "$day" +%s)
source="$output/$day/browser_latest-$station-$epoch.h5"
[[ -f "$source" ]] || exit 0
fingerprint=$(stat -c '%s:%Y:%i' "$source")
[[ ! -f "$state/sent" || "$(cat "$state/sent")" != "$fingerprint" ]] || exit 0
snapshot="$state/${source##*/}"
rm -f "$snapshot"
trap 'rm -f "$snapshot"' EXIT
# The producer publishes with atomic rename. Hold this inode while it advances.
ln "$source" "$snapshot" 2>/dev/null || cp "$source" "$snapshot"
(( $(stat -c %s "$snapshot") <= 4194304 )) || exit 65
curl --fail --silent --show-error --connect-timeout 15 --max-time 120 \
    --retry 0 --limit-rate 16k -F "file=@$snapshot" https://juha.no/upload_h5.php
printf '%s\n' "$fingerprint" > "$state/sent"
echo "TGO latest ionograms published" >&2
