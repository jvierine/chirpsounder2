#!/usr/bin/env bash
# Replays immutable receiver-local baseband caches; no ringbuffer reads.
set -euo pipefail
config=${1:?config}; capture=${2:?capture directory}; binary=${3:?native verifier}
for epoch in 1794065438 1794065464 1794065489 1794065515; do
 "$binary" "$config" --iq "$capture/digisonde_code_matches-$epoch.h5"
done
