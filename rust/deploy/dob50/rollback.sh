#!/usr/bin/env bash
set -euo pipefail
[[ $(hostname -s) == tgo-mic1 ]] || exit 64
base=/home/tgo/src/dob50-trial
systemctl --user stop iono-prn@Montelibretti iono-rust-detect dob50-spectrum dob50-acquisition dob50-clean || true
systemctl --user disable dob50-acquisition dob50-clean dob50-spectrum || true
# The old housekeeping service does not know about the experimental raw ring.
rm -rf /dev/shm/hf50
rm -f ~/.config/systemd/user/iono-prn@Montelibretti.service.d/zzzz-dob50.conf
rm -f ~/.config/systemd/user/iono-rust-detect.service.d/zzzz-dob50.conf
rm -f ~/.config/systemd/user/iono-browser-arrays.service.d/zzzzzzz-dob50.conf
systemctl --user daemon-reload
systemctl --user restart iono-browser-arrays
systemctl --user enable --now chirpsounder_dombas.service
mapfile -t units < "$base/install-state/previously-active.txt"
systemctl --user start "${units[@]}"
printf 'Rolled DOB back to its original 25 MHz recorder and profiles.\n'
