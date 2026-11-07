#!/usr/bin/env bash
# Install only the added station worker; leave acquisition and existing DSP alone.
set -euo pipefail
config=${1:?absolute path to reviewed station thule.json}
unit=${2:?absolute path to iono-digisonde@.service}
[[ "$config" = /* && "$unit" = /* ]]
"$HOME/bin/iono-receiver" check-config "$config"
mkdir -p "$HOME/.config/chirpsounder2" "$HOME/.config/systemd/user" "$HOME/.local/state/chirpsounder2-rollout/pituffik-20261107"
backup="$HOME/.local/state/chirpsounder2-rollout/pituffik-20261107"
for file in "$HOME/.config/chirpsounder2/thule.json" "$HOME/.config/systemd/user/iono-digisonde@.service"; do
  [[ ! -f "$file" || -f "$backup/$(basename "$file").before" ]] || cp -p "$file" "$backup/$(basename "$file").before"
done
install -m644 "$config" "$HOME/.config/chirpsounder2/thule.json"
# Preserve an existing station template and its local limits.
[[ -f "$HOME/.config/systemd/user/iono-digisonde@.service" ]] || install -m644 "$unit" "$HOME/.config/systemd/user/iono-digisonde@.service"
mkdir -p "$HOME/.config/systemd/user/iono-digisonde@thule.service.d"
cat > "$HOME/.config/systemd/user/iono-digisonde@thule.service.d/priority.conf" <<'EOF'
[Service]
Nice=10
CPUWeight=50
CPUQuota=100%
MemoryMax=384M
EOF
systemctl --user daemon-reload
systemctl --user enable --now iono-digisonde@thule.service
systemctl --user show iono-digisonde@thule.service -p ActiveState -p MainPID -p CPUQuotaPerSecUSec
