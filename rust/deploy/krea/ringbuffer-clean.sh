#!/usr/bin/env bash
# Only disposable voltage files from this receiver's private RAM ring are pruned.
# Retained science products under /data0 are never touched.
set -euo pipefail
ring=/dev/shm/hf25/ch0
while true; do
    if [[ -d "$ring" ]]; then
        find "$ring" -mindepth 2 -maxdepth 2 -type f -name 'rf@*.h5' \
            -mmin +0.333333 -delete
        find "$ring" -mindepth 1 -maxdepth 1 -type d -empty -delete
    fi
    sleep 1
done
