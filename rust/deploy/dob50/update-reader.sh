#!/usr/bin/env bash
# Update the dedicated DOB50 consumers; never restart acquisition.
set -euo pipefail
[[ $(hostname -s) == tgo-mic1 ]] || exit 64
base=/home/tgo/src/dob50-trial
stage=${1:?usage: update-reader.sh STAGED_DIRECTORY}
[[ -d "$stage" && -f "$stage/SHA256SUMS" ]] || exit 65
(cd "$stage" && sha256sum --check SHA256SUMS)
units=(iono-prn@Montelibretti.service iono-rust-detect.service dob50-spectrum.service)
for unit in "${units[@]}"; do systemctl --user is-active --quiet "$unit"; done
before=$(systemctl --user show dob50-acquisition -p MainPID --value)
[[ $before != 0 ]] || exit 66
backup="$base/install-state/reader-$(date -u +%Y%m%dT%H%M%S)"
mkdir -p "$backup"
for binary in iono-prn iono-receiver; do
    cp -p "$base/$binary" "$backup/$binary"
    install -m 755 "$stage/$binary" "$base/$binary.next"
done
restore() {
    for binary in iono-prn iono-receiver; do
        install -m 755 "$backup/$binary" "$base/$binary.next"
        mv "$base/$binary.next" "$base/$binary"
    done
    systemctl --user restart "${units[@]}"
}
for binary in iono-prn iono-receiver; do mv "$base/$binary.next" "$base/$binary"; done
if ! systemctl --user restart "${units[@]}"; then restore; exit 67; fi
sleep 3
for unit in "${units[@]}"; do
    if ! systemctl --user is-active --quiet "$unit"; then restore; exit 68; fi
done
after=$(systemctl --user show dob50-acquisition -p MainPID --value)
[[ $before == "$after" ]] || { echo 'Acquisition PID changed independently; inspect health' >&2; exit 69; }
sha256sum "$base/iono-prn" "$base/iono-receiver" > "$backup/installed.sha256"
echo "Updated consumers; acquisition PID $after unchanged; backup $backup"
