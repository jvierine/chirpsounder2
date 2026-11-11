#!/usr/bin/env bash
# Debounce a physical link, then invoke a trusted callback with up/down.
# The callback is retried on failure; loss of the carrier file means down.
set -euo pipefail
carrier=${1:?carrier file}; callback=${2:?callback}; interval=${3:-2}
candidate=; applied=; stable=0
while true; do
    value=$(cat "$carrier" 2>/dev/null || true)
    state=down; [[ "$value" != 1 ]] || state=up
    if [[ "$state" == "$candidate" ]]; then stable=$((stable+1)); else candidate=$state; stable=1; fi
    required=3; [[ "$state" != up ]] || required=2
    if [[ "$state" != "$applied" && "$stable" -ge "$required" ]]; then
        if "$callback" "$state"; then
            applied=$state
            echo "KHO USRP link $state: handover action completed"
        fi
    fi
    sleep "$interval"
done
