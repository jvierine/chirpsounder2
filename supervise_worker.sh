#!/usr/bin/env bash
# Keep one worker alive without restarting the receiver or its ringbuffer.
set -u
name="$1"
shift
[[ "$name" =~ ^[a-zA-Z0-9_-]+$ ]] || exit 2
exec 9>"/tmp/chirpsounder-${UID}-${name}.lock"
flock -n 9 || exit 0
child=""
stop_worker() {
    trap - TERM INT HUP
    if [[ -n "$child" ]]; then
        kill -TERM "$child" 2>/dev/null || true
        wait "$child" 2>/dev/null || true
    fi
    exit 0
}
trap stop_worker TERM INT HUP
while true; do
    "$@" 9>&- &
    child=$!
    wait "$child"
    result=$?
    child=""
    printf '%s worker %s exited (%s); restarting in 5 seconds\n' "$(date -u +%FT%TZ)" "$name" "$result" >&2
    sleep 5 &
    child=$!
    wait "$child"
    child=""
done
