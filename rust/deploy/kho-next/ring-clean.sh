#!/usr/bin/env bash
set -euo pipefail
while true; do
    if [[ -d /dev/shm/hf25/ch0 ]]; then
        # Only disposable voltage is pruned. Native science products are kept.
        find /dev/shm/hf25/ch0 -mindepth 2 -maxdepth 2 -type f \( -name 'rf@*.h5' -o -name 'tmp*rf*.h5' \) -mmin +1.5 -delete
        find /dev/shm/hf25/ch0 -mindepth 1 -maxdepth 1 -type d -empty -delete
    fi
    sleep 1
done
