#!/usr/bin/env bash
set -euo pipefail
state="$HOME/.local/state/iono-kho-latest"
mkdir -p "$state"
last=
while true; do
    day=$(date -u +%F); epoch=$(date -u -d "$day" +%s)
    source="/data0/$day/browser_latest-KHO-$epoch.h5"
    if [[ -f "$source" ]]; then
        fingerprint=$(stat -c '%s:%Y:%i' "$source")
        if [[ "$fingerprint" != "$last" ]]; then
            snapshot="$state/${source##*/}"
            rm -f "$snapshot"
            ln "$source" "$snapshot" 2>/dev/null || cp "$source" "$snapshot"
            if curl --fail --silent --show-error --connect-timeout 15 --max-time 120 \
                --retry 0 --limit-rate 32k -F "file=@$snapshot" https://juha.no/upload_h5.php; then
                last=$fingerprint
            fi
            rm -f "$snapshot"
        fi
    fi
    sleep 30
done
