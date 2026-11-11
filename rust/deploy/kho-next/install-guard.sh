#!/usr/bin/env bash
set -euo pipefail
[[ $(id -u) == 0 ]]
role=${1:?new or old}
here=$(cd "$(dirname "$0")" && pwd)
case "$role" in
    new) account=administrator; interface=usrp0 ;;
    old) account=j; interface=eno1 ;;
    *) exit 64 ;;
esac
test -r "/sys/class/net/$interface/carrier"
loginctl enable-linger "$account"
install -d /usr/local/libexec/kho
install -m755 "$here/link-watch.sh" /usr/local/libexec/kho/link-watch.sh
install -m755 "$here/link-action-$role.sh" /usr/local/libexec/kho/link-action.sh
if [[ "$role" == old ]]; then
    uid=$(id -u j)
    ctl=(runuser -u j -- env "XDG_RUNTIME_DIR=/run/user/$uid" systemctl --user)
    units=(iono-browser-arrays iono-prn@Kiruna iono-prn@Uppsala iono-prn@Montelibretti
           iono-verso iono-digisonde@thule iono-rust-digisonde iono-rust-lfm
           iono-rust-detect iono-rust-spectrum)
    mkdir -p /root/kho-before-handover
    if [[ ! -f /root/kho-before-handover/enabled-units.txt ]]; then
        "${ctl[@]}" list-unit-files > /root/kho-before-handover/enabled-units.txt
        systemctl is-enabled chirpsounder-kho > /root/kho-before-handover/acquisition-enabled.txt || true
    fi
    # Disable only their direct boot hooks; active reception is undisturbed.
    # The carrier watcher now owns boot/start and cable-back recovery.
    "${ctl[@]}" disable "${units[@]}"
    systemctl disable chirpsounder-kho
fi
cat > /etc/systemd/system/kho-link-handover.service <<EOF
[Unit]
Description=KHO cable handover guard ($role receiver)
After=network.target user@$(id -u "$account").service
Wants=user@$(id -u "$account").service
StartLimitIntervalSec=0
[Service]
ExecStart=/usr/local/libexec/kho/link-watch.sh /sys/class/net/$interface/carrier /usr/local/libexec/kho/link-action.sh
Restart=always
RestartSec=5
[Install]
WantedBy=multi-user.target
EOF
systemctl daemon-reload
systemctl enable --now kho-link-handover.service
