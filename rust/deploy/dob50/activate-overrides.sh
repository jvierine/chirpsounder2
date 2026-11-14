#!/usr/bin/env bash
set -euo pipefail
[[ $(hostname -s) == tgo-mic1 ]] || exit 64
for unit in iono-prn@Montelibretti iono-rust-detect; do
    dir="$HOME/.config/systemd/user/$unit.service.d"
    # Existing zzzz-chirpsounder2.conf must not override this explicit trial.
    if [[ -e "$dir/zz-dob50.conf" ]]; then mv "$dir/zz-dob50.conf" "$dir/zzzz-dob50.conf"; fi
done
systemctl --user daemon-reload
systemctl --user restart iono-prn@Montelibretti iono-rust-detect
systemctl --user show iono-prn@Montelibretti -p ExecStart
