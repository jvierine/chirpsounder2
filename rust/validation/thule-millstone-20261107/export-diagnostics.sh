#!/usr/bin/env bash
set -euo pipefail
binary=${1:?verifier}; source=${2:?IQ cache directory}; shift 2
mkdir -p /tmp/thule-diagnostics-summary
files=()
for epoch in "$@"; do
 "$binary" --summary "$source/digisonde_code_matches-$epoch.h5" "/tmp/thule-diagnostics-summary/digisonde_code_matches-$epoch.h5"
 files+=("digisonde_code_matches-$epoch.h5")
done
tar -czf /tmp/thule-diagnostics-summary.tgz -C /tmp/thule-diagnostics-summary "${files[@]}"
