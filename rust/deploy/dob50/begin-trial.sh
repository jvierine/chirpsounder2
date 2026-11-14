#!/usr/bin/env bash
set -euo pipefail
[[ $(hostname -s) == tgo-mic1 ]] || exit 64
base=/home/tgo/src/dob50-trial
# An independent timer restores known-good acquisition if supervision is lost.
systemd-run --user --unit=dob50-revert --on-active=12min /bin/bash "$base/rollback.sh"
mapfile -t units < "$base/install-state/previously-active.txt"
systemctl --user stop "${units[@]}" chirpsounder_dombas.service
systemctl --user start dob50-clean dob50-acquisition
for ((i=0;i<30;i++)); do
    if [[ -d /dev/shm/hf25/ch0 ]] && find /dev/shm/hf25/ch0 -name 'rf@*.h5' -print -quit | grep -q .; then
        systemctl --user start "${units[@]}" dob50-spectrum
        exit 0
    fi
    sleep 1
done
"$base/rollback.sh"
exit 1
