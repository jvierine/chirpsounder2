#!/usr/bin/env bash
set -euo pipefail
[[ $(hostname -s) == tgo-mic1 ]] || exit 64
while true; do
    for spec in 'hf50 40' 'hf25 90'; do
        read -r name age <<< "$spec"
        root=/dev/shm/$name/ch0
        if [[ -d "$root" ]]; then
            find "$root" -type f -name 'rf@*.h5' ! -newermt "$age seconds ago" -delete
        fi
    done
    sleep 1
done
