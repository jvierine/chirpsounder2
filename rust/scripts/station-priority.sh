#!/usr/bin/env bash
# Receiver scheduling policy; no acquisition restarts or timing changes.
# Global search has a separate CPU budget.
set -euo pipefail
if [[ ${1:-} == --install ]]; then
  while read -r unit _; do
    case "$unit" in
      chirpsounder_*.service) weight=2000; shares=20480; nice=0;;
      iono-rust-digisonde.service|iono-pfb.service) weight=1000; shares=10240; nice=0;;
      iono-rust-lfm.service) weight=600; shares=6144; nice=2;;
      iono-rust-detect.service) weight=1; shares=2; nice=19;;
      iono-prn@*.service|iono-verso.service|iono-digisonde@*.service|iono-rust-juliusruh-*.service) weight=50; shares=512; nice=10;;
      *) continue;;
    esac
    dir="$HOME/.config/systemd/user/$unit.d"
    mkdir -p "$dir"
    if [[ -f /sys/fs/cgroup/cgroup.controllers ]]; then
      property=CPUWeight; value=$weight
      # Remove only the exact legacy runtime override this installer generated.
      legacy="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/systemd/user.control/$unit.d/50-CPUShares.conf"
      if [[ -f "$legacy" ]] && grep -qx "CPUShares=$shares" "$legacy"; then rm -- "$legacy"; fi
    else
      property=CPUShares; value=$shares
    fi
    printf '[Service]\n%s=%s\nNice=%s\n' "$property" "$value" "$nice" > "$dir/zz-station-priority.conf"
    pid=$(systemctl --user show "$unit" -p MainPID --value)
    if [[ "$nice" -gt 0 && "$pid" =~ ^[1-9][0-9]*$ ]]; then
      current=$(ps -o ni= -p "$pid" | tr -d ' ')
      [[ -z "$current" || "$current" -ge "$nice" ]] || renice -n "$nice" -p "$pid" >/dev/null
    fi
    # Resource weights change live. Nice is also applied to existing children below.
    systemctl --user set-property --runtime "$unit" "$property=$value"
  done < <(systemctl --user list-units --type=service --state=running --plain --no-legend)
  systemctl --user daemon-reload
fi
# Legacy station launchers put several children in one service. Preserve the
# receiver ordering there too; never touch unrelated Python or user processes.
while read -r pid niceness args; do
  target=
  case " $args " in
    *' receive_digisonde.py '*|*'/receive_digisonde.py '*|*' iono-receiver digisonde '*) target=0;;
    *' calc_ionograms.py '*|*'/calc_ionograms.py '*|*'/iono-receiver lfm '*) target=2;;
    *' detect_chirps.py '*|*'/detect_chirps.py '*|*'/iono-receiver detect '*) target=19;;
    *'/iono-prn '*|*'/iono-verso '*) target=10;;
    *'/iono-pfb '*) target=0;;
    *' iono_housekeeping.py '*|*'/iono_housekeeping.py '*|*' station_monitor.py '*|*' -u station_monitor.py '*|*' plot_rf_spec.py '*|*' -u plot_rf_spec.py '*|*' plot_ionograms.py '*|*' plot_digisonde.py '*|*' plot_rtf.py '*|*' plot_detectionfiles.py '*|*' sync_iono_data.py '*) target=15;;
  esac
  [[ -n "$target" && "$niceness" -lt "$target" ]] || continue
  renice -n "$target" -p "$pid" >/dev/null 2>&1 || true
done < <(ps -u "$(id -u)" -o pid=,ni=,args=)
