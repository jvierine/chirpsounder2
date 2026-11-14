#!/usr/bin/env bash
# DOB only: bounded hardware test with automatic restoration of original services.
set -euo pipefail
[[ $(hostname -s) == tgo-mic1 ]] || exit 64
base=/home/tgo/src/dob50-trial
state="$base/probe-state"
mkdir -p "$state"
mapfile -t units < <(systemctl --user list-units 'iono-*' --type=service --state=active --no-legend --plain | awk '$1!="iono-delta-publish.service"{print $1}')
printf '%s\n' "${units[@]}" > "$state/previously-active.txt"
restore(){
    trap - EXIT
    systemctl --user start chirpsounder_dombas.service
    systemctl --user start "${units[@]}"
}
trap restore EXIT
systemctl --user stop "${units[@]}" chirpsounder_dombas.service
if pgrep -x rx_uhd_ext_gps; then echo 'Old recorder did not stop'; exit 1; fi
# Only volatile raw-voltage caches; science products on /data0 are untouched.
rm -rf /dev/shm/hf25 /dev/shm/hf50-probe /dev/shm/hf25-probe
mkdir -p /data0/recorder-health/dob50-probe
"$base/rx_uhd_ext_gps" --mode 50mhz --outdir /dev/shm/hf50-probe \
    --compat25-outdir /dev/shm/hf25-probe --stats-dir /data0/recorder-health/dob50-probe \
    --seconds 45 > "$base/probe.log" 2>&1
# Keep one native second of each stream locally for reproducible decoder checks.
for kind in hf50 hf25; do
    source=/dev/shm/$kind-probe/ch0
    dest=/data0/recorder-health/dob50-probe/$kind/ch0
    mkdir -p "$dest"
    cp "$source/drf_properties.h5" "$dest/"
    file=$(find "$source" -name 'rf@*.h5' -type f | sort | head -1)
    [[ -n "$file" ]]
    relative=${file#"$source/"}
    mkdir -p "$dest/$(dirname "$relative")"
    cp "$file" "$dest/$relative"
done
rm -rf /dev/shm/hf50-probe /dev/shm/hf25-probe
printf 'probe completed and archived\n' > "$state/result.txt"
