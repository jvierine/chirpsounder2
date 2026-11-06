#!/usr/bin/env bash
# Install one finite, reboot-persistent three-carrier capture sequence.
set -euo pipefail
start=${1:?future UTC Unix epoch}
[[ "$start" =~ ^[0-9]{10}$ && $(date +%s) -lt "$start" ]]
config_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
helper="$HOME/bin/iono-digisonde-verify"
[[ -x "$helper" ]]
unit_dir="$HOME/.config/systemd/user"
mkdir -p "$unit_dir"
timers=()
for chunk in {0..13}; do
  name="iono-dob-daytime-${start}-${chunk}"
  first=$((chunk*8))
  last="$((first+7)).84"
  utc=$(date -u -d "@$((start+(chunk+1)*8+5))" '+%Y-%m-%d %H:%M:%S UTC')
  cat > "$unit_dir/$name.service" <<UNIT
[Unit]
Description=Finite DOB coded-sounder timing capture ${chunk}
[Service]
Type=oneshot
ExecStart="$helper" "$config_dir/capture.json" --grid $start $first $last .16 4000000 6000000 1000000
Nice=15
CPUQuota=75%
MemoryMax=256M
RuntimeMaxSec=90
LimitCORE=0
UNIT
  cat > "$unit_dir/$name.timer" <<UNIT
[Unit]
Description=Finite DOB daytime validation window ${chunk}
[Timer]
OnCalendar=$utc
AccuracySec=100ms
Persistent=true
Unit=$name.service
[Install]
WantedBy=timers.target
UNIT
  timers+=("$name.timer")
done
systemctl --user daemon-reload
systemctl --user enable --now "${timers[@]}"
systemctl --user list-timers 'iono-dob-daytime-*' --no-pager
