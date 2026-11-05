#!/usr/bin/env bash
# Run independently of acquisition and array generation. Failed uploads retain
# one immutable snapshot and rsync's partial blocks, instead of starting over.
set -euo pipefail
station=${1:?station required}
output=${2:?output directory required}
destination=${3:-j@juha.no}
cooldown=${4:-300}
[[ "$cooldown" =~ ^[0-9]+$ ]] && (( cooldown >= 60 && cooldown <= 3600 )) || exit 64
[[ "$station" =~ ^(DOB|KHO|TGO|W2NAF|KREA)$ ]] || exit 64
pending="${XDG_STATE_HOME:-$HOME/.local/state}/iono-publish/$station"
mkdir -p "$pending"
exec 9>"$pending/publish.lock"
flock -n 9 || exit 0
while true; do
    day=$(date -u +%F)
    epoch=$(date -u -d "$day" +%s)
    name="browser_day-$station-$epoch.h5"
    source="$output/$day/$name"
    snapshot="$pending/$name"
    # A failed previous-day transfer must not queue a historic backfill.
    for old in "$pending"/browser_day-"$station"-*.h5; do
        [[ "$old" == "$snapshot" ]] || rm -f "$old"
    done
    if [[ ! -f "$snapshot" ]]; then
        if [[ ! -f "$source" ]]; then sleep 60; continue; fi
        ln "$source" "$snapshot" 2>/dev/null || cp "$source" "$snapshot"
    fi
    if rsync -z --times --partial-dir=.rsync-partial --bwlimit=16 --timeout=600 --stats \
        -e 'ssh -oBatchMode=yes -oStrictHostKeyChecking=yes -oConnectTimeout=30 -oServerAliveInterval=60 -oServerAliveCountMax=10' \
        "$snapshot" "$destination:$name"; then
        rm -f "$snapshot"
        sleep "$cooldown"
    else
        echo "Transfer interrupted; keeping the snapshot and partial blocks for retry." >&2
        sleep 120
    fi
done
