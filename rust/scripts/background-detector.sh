#!/usr/bin/env bash
# One logical CPU of aggregate search time; acquisition never restarts.
set -euo pipefail
unit=iono-rust-detect.service
systemctl --user cat "$unit" >/dev/null
dir="$HOME/.config/systemd/user/$unit.d"
mkdir -p "$dir"
if [[ -f /sys/fs/cgroup/cgroup.controllers ]]; then
  weight=CPUWeight=1
else
  weight=CPUShares=2
fi
printf '[Service]\nNice=19\nCPUQuota=100%%\n%s\n' "$weight" > "$dir/zzzz-background-budget.conf"
systemctl --user daemon-reload
systemctl --user set-property "$unit" CPUQuota=100% "$weight"
pid=$(systemctl --user show "$unit" -p MainPID --value)
if [[ "$pid" =~ ^[1-9][0-9]*$ ]]; then
  for task in /proc/"$pid"/task/*; do
    renice -n 19 -p "${task##*/}" >/dev/null
  done
fi
systemctl --user show "$unit" -p MainPID -p CPUQuotaPerSecUSec -p CPUWeight -p CPUShares -p Nice
