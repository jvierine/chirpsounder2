#!/usr/bin/env bash
# Bounded receiver-local discovery. HDF5 diagnostics stay on the station.
set -euo pipefail
config=${1:?receiver search config}
# Serialize rounds so a slow scan never creates a growing queue.
mkdir -p "$HOME/.local/state/chirpsounder2"
exec 9>"$HOME/.local/state/chirpsounder2/digisonde-watch.lock"
flock -n 9 || exit 0
result=$("$HOME/bin/iono-digisonde-search" "$config" --lookback 180)
file=${result##*; }
test -f "$file"
"$HOME/bin/iono-digisonde-verify" "$config" "$file" 256
