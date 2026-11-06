#!/usr/bin/env bash
# Migrate only analysis workers; never restart the USRP recorder.
set -euo pipefail
station=${1:?station DOB or KHO}
bundle=${2:?absolute staged bundle directory}
binary=${3:?reviewed compressed native binary}
case "$station" in
  DOB) name=dombas; monitor_python="$HOME/ionosonde/bin/python";;
  KHO) name=kho; monitor_python="$HOME/src/chirpsounder2/.venv/bin/python3";;
  *) echo 'unsupported station' >&2; exit 2;;
esac
[[ "$bundle" = /* && -d "$bundle" ]]
repo="$HOME/src/chirpsounder2"
backup="$HOME/.local/state/chirpsounder2-rollout/native-dsp-20261106"
mkdir -p "$backup" "$HOME/bin" "$HOME/.config/systemd/user" "$HOME/.config/chirpsounder2"
recorder_before=$(pgrep -f '^./rx_uhd_ext_gps ' | sort)
[[ -n "$recorder_before" ]]
for file in "$repo/examples/marieluise/$name.sh" "$repo/examples/marieluise/$name.ini" "$HOME/bin/iono-receiver"; do
  dest="$backup/$(basename "$file").before"
  [[ -e "$dest" ]] || cp -p "$file" "$dest"
done
for file in "$bundle/units/"*.service; do
  dest="$HOME/.config/systemd/user/$(basename "$file")"
  [[ ! -e "$dest" || -e "$backup/$(basename "$file").before" ]] || cp -p "$dest" "$backup/$(basename "$file").before"
  install -m644 "$file" "$dest"
done
for file in "$bundle/config/"*.json; do
  dest="$HOME/.config/chirpsounder2/$(basename "$file")"
  [[ ! -e "$dest" || -e "$backup/$(basename "$file").before" ]] || cp -p "$dest" "$backup/$(basename "$file").before"
  if [[ $(basename "$file") == receiver-timing.json && -e "$dest" ]]; then
    cmp -s "$file" "$dest" || { echo 'Existing receiver timing differs; refusing to overwrite' >&2; exit 2; }
  fi
  install -m644 "$file" "$dest"
done
gzip -dc "$binary" > "$HOME/bin/iono-receiver.new"
chmod 755 "$HOME/bin/iono-receiver.new"
for config in "$bundle/config/"*.json; do
  [[ $(basename "$config") == receiver-timing.json ]] && continue
  "$HOME/bin/iono-receiver.new" check-config "$config"
done
mv "$HOME/bin/iono-receiver.new" "$HOME/bin/iono-receiver"
install -m755 "$bundle/$name.sh" "$repo/examples/marieluise/$name.sh"
install -m644 "$bundle/$name.ini" "$repo/examples/marieluise/$name.ini"
# Match only the retired station scripts; do not touch acquisition, PRN, VERSO,
# the Rust browser-array worker, housekeeping, or unrelated Python processes.
mapfile -t retired < <(ps -u "$(id -u)" -o pid=,args= | awk '$0 ~ /[ /](receive_digisonde|detect_chirps|detections2metadata|calc_ionograms|plot_rtf|plot_detectionfiles|plot_ionograms|sync_iono_data|station_monitor)\.py([ ]|$)/ {print $1}')
if ((${#retired[@]})); then kill -TERM "${retired[@]}"; fi
systemctl --user daemon-reload
if [[ "$station" == KHO ]]; then
  mkdir -p "$HOME/.config/systemd/user/iono-rust-detect.service.d"
  printf '[Service]\nMemoryMax=2G\n' > "$HOME/.config/systemd/user/iono-rust-detect.service.d/capacity.conf"
  systemctl --user daemon-reload
fi
units=(iono-rust-digisonde.service iono-rust-lfm.service iono-rust-detect.service iono-rust-spectrum.service)
if [[ "$station" == DOB ]]; then
  units+=(iono-digisonde@juliusruh-day.service iono-digisonde@juliusruh-night.service iono-digisonde@dourbes.service iono-digisonde@dourbes-night.service iono-digisonde@dourbes-regular-05.service iono-digisonde@dourbes-night-regular-05.service iono-digisonde@dourbes-regular-10.service iono-digisonde@dourbes-night-regular-10.service iono-digisonde@chilton.service)
fi
systemctl --user enable --now "${units[@]}"
nohup "$monitor_python" "$repo/station_monitor.py" --config "$repo/examples/marieluise/$name.ini" --period-s 60 > "$repo/logs/station_monitor.log" 2>&1 < /dev/null &
recorder_after=$(pgrep -f '^./rx_uhd_ext_gps ' | sort)
[[ "$recorder_before" == "$recorder_after" ]]
printf 'Recorder unchanged: %s\n' "$recorder_after"
sha256sum "$HOME/bin/iono-receiver"
systemctl --user show "${units[@]}" -p Id -p ActiveState -p MainPID -p NRestarts
