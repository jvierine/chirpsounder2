#!/usr/bin/env bash
# Keep the old receiver available for a cable-back rollback, but never publish
# from it while its radio cable is disconnected. No unit is restarted on up.
set -euo pipefail
uid=$(id -u j)
ctl=(runuser -u j -- env "XDG_RUNTIME_DIR=/run/user/$uid" systemctl --user)
units=(iono-browser-arrays iono-prn@Kiruna iono-prn@Uppsala iono-prn@Montelibretti
       iono-verso iono-digisonde@thule iono-rust-digisonde iono-rust-lfm
       iono-rust-detect iono-rust-spectrum)
case "${1:?up or down}" in
    up)
        systemctl start chirpsounder-kho.service
        "${ctl[@]}" start "${units[@]}"
        ;;
    down)
        "${ctl[@]}" stop "${units[@]}"
        systemctl stop chirpsounder-kho.service
        ;;
    *) exit 64 ;;
esac
