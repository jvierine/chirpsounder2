#!/usr/bin/env bash
# Update DOB reader consumers atomically; acquisition and profiles are untouched.
set -euo pipefail
[[ $(hostname -s) == tgo-mic1 ]] || exit 64
stage=${1:?usage: update-io.sh STAGED_DIRECTORY}
base=/home/tgo/src/dob50-trial
(cd "$stage" && sha256sum --check SHA256SUMS)
before=$(systemctl --user show dob50-acquisition -p MainPID --value)
[[ $before != 0 ]] || exit 65
units=()
while read -r unit _; do
    command=$(systemctl --user show "$unit" -p ExecStart --value)
    case "$command" in
        *'/home/tgo/bin/iono-prn '*|*'/home/tgo/bin/iono-receiver '*|*'/home/tgo/bin/iono-verso '*|*'/home/tgo/src/dob50-trial/iono-prn '*|*'/home/tgo/src/dob50-trial/iono-receiver '*) units+=("$unit");;
    esac
done < <(systemctl --user list-units 'iono-*' 'dob50-*' --type=service --state=running --plain --no-legend)
(( ${#units[@]} > 0 )) || exit 66
backup="$base/install-state/io-$(date -u +%Y%m%dT%H%M%S)"
mkdir -p "$backup/bin" "$backup/dob50-trial"
printf '%s\n' "${units[@]}" > "$backup/units.txt"
targets=("$HOME/bin/iono-prn" "$HOME/bin/iono-receiver" "$HOME/bin/iono-verso" "$base/iono-prn" "$base/iono-receiver")
for target in "${targets[@]}"; do
    parent=${target%/*}; parent=${parent##*/}
    cp -p "$target" "$backup/$parent/${target##*/}"
    install -m 755 "$stage/${target##*/}" "$target.next"
done
restore() {
    for target in "${targets[@]}"; do
        parent=${target%/*}; parent=${parent##*/}
        install -m 755 "$backup/$parent/${target##*/}" "$target.next"
        mv "$target.next" "$target"
    done
    systemctl --user restart "${units[@]}"
}
for target in "${targets[@]}"; do mv "$target.next" "$target"; done
if ! systemctl --user restart "${units[@]}"; then restore; exit 67; fi
sleep 3
for unit in "${units[@]}"; do
    if ! systemctl --user is-active --quiet "$unit"; then restore; exit 68; fi
done
after=$(systemctl --user show dob50-acquisition -p MainPID --value)
[[ $before == "$after" ]] || { echo 'Acquisition PID changed independently; inspect health' >&2; exit 69; }
for target in "${targets[@]}"; do sha256sum "$target"; done > "$backup/installed.sha256"
cat "$backup/units.txt" "$backup/installed.sha256"
echo "Acquisition PID $after unchanged; backups $backup"
