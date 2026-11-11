#!/usr/bin/env bash
set -euo pipefail
while true; do
    if [[ -r /dev/shm/hf25/ch0/drf_properties.h5 ]] &&
       [[ -n "$(find /dev/shm/hf25/ch0 -mindepth 2 -maxdepth 2 -type f -name 'rf@*.h5' -mmin -0.5 -print -quit 2>/dev/null)" ]]; then
        exit 0
    fi
    sleep 2
done
