#!/usr/bin/env bash
set -euo pipefail
uid=$(id -u administrator)
ctl=(runuser -u administrator -- env "XDG_RUNTIME_DIR=/run/user/$uid" systemctl --user)
case "${1:?up or down}" in
    up) "${ctl[@]}" start --no-block iono-kho.target ;;
    down) "${ctl[@]}" stop iono-kho.target ;;
    *) exit 64 ;;
esac
