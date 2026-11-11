#!/usr/bin/env bash
# Run only after synthetic, native integration and same-RF station checks pass.
# Never replaces acquisition/configuration or restarts LFM/search workers.
set -euo pipefail
[[ ${1:-} == --enable && $# == 3 ]] || { echo 'Usage: install-shared-pfb.sh --enable RELEASE_BIN_DIR SOURCE_REVISION' >&2; exit 2; }
binaries=$(cd "$2" && pwd)
revision=$3
[[ $revision =~ ^[0-9a-f]{40}$ ]] || { echo 'Need exact source commit' >&2; exit 2; }
root=$(cd "$(dirname "$0")/../.." && pwd)
units_dir="$HOME/.config/systemd/user"
config="$HOME/.config/chirpsounder2/pfb.json"
state="$HOME/.local/state/chirpsounder2/pfb-rollback-$(date -u +%Y%m%dT%H%M%SZ)"
[[ ! -e "$units_dir/iono-pfb.service" && ! -e "$config" ]] || { echo 'An existing PFB installation needs inspection before replacement' >&2; exit 1; }
for name in iono-receiver iono-prn iono-verso iono-digisonde-search iono-digisonde-verify iono-pfb; do
    [[ -x "$binaries/$name" ]] || { echo "Missing candidate: $name" >&2; exit 1; }
done
mkdir -p "$state/bin" "$HOME/bin" "$(dirname "$config")"
systemctl --user list-units --all --plain --no-legend \
    'iono-prn@*.service' 'iono-verso.service' 'iono-rust-digisonde.service' \
    'iono-rust-juliusruh-*.service' 'iono-digisonde@*.service' \
    | awk '{print $1}' > "$state/units"
[[ -s "$state/units" ]] || { echo 'No coded-sounder services found' >&2; exit 1; }
: > "$state/active"
while read -r unit; do
    [[ ! -e "$units_dir/$unit.d/90-shared-pfb.conf" ]] || { echo "Existing override: $unit" >&2; exit 1; }
    if systemctl --user is-active --quiet "$unit"; then echo "$unit" >> "$state/active"; fi
done < "$state/units"
for name in iono-receiver iono-prn iono-verso iono-digisonde-search iono-digisonde-verify iono-pfb; do
    if [[ -e "$HOME/bin/$name" ]]; then cp -p "$HOME/bin/$name" "$state/bin/$name"; fi
done
printf '%s\n' "$revision" > "$state/candidate-revision"
cat > "$state/rollback.sh" <<'ROLLBACK'
#!/usr/bin/env bash
set -euo pipefail
state=$(cd "$(dirname "$0")" && pwd)
systemctl --user stop iono-pfb.service || true
while read -r unit; do rm -f "$HOME/.config/systemd/user/$unit.d/90-shared-pfb.conf"; done < "$state/units"
for name in iono-receiver iono-prn iono-verso iono-digisonde-search iono-digisonde-verify iono-pfb; do
    if [[ -e "$state/bin/$name" ]]; then
        install -m755 "$state/bin/$name" "$HOME/bin/$name.restore"
        mv -f "$HOME/bin/$name.restore" "$HOME/bin/$name"
    elif [[ $name == iono-pfb ]]; then rm -f "$HOME/bin/$name"; fi
done
systemctl --user disable iono-pfb.service || true
rm -f "$HOME/.config/systemd/user/iono-pfb.service" "$HOME/.config/chirpsounder2/pfb.json"
systemctl --user daemon-reload
while read -r unit; do systemctl --user restart "$unit"; done < "$state/active"
echo 'Restored the previous receiver binaries and coded-sounder service settings.'
ROLLBACK
chmod 700 "$state/rollback.sh"
# Undo a partial install automatically; rollback is also retained for later use.
trap 'bash "$state/rollback.sh"' ERR
for name in iono-receiver iono-prn iono-verso iono-digisonde-search iono-digisonde-verify iono-pfb; do
    install -m755 "$binaries/$name" "$HOME/bin/$name.pfb-new"
    mv -f "$HOME/bin/$name.pfb-new" "$HOME/bin/$name"
done
install -m644 "$root/rust/deploy/pfb/iono-pfb.service" "$units_dir/iono-pfb.service"
install -m600 "$root/rust/deploy/pfb/tgo.json" "$config"
while read -r unit; do
    mkdir -p "$units_dir/$unit.d"
    cat > "$units_dir/$unit.d/90-shared-pfb.conf" <<'UNIT'
[Unit]
Wants=iono-pfb.service
After=iono-pfb.service
[Service]
Environment=IONO_PFB_SOCKET=%t/iono-pfb.sock
UNIT
done < "$state/units"
systemctl --user daemon-reload
systemctl --user enable --now iono-pfb.service
for _ in {1..30}; do [[ -S "${XDG_RUNTIME_DIR:?}/iono-pfb.sock" ]] && break; sleep 0.1; done
[[ -S "$XDG_RUNTIME_DIR/iono-pfb.sock" ]]
while read -r unit; do systemctl --user restart "$unit"; done < "$state/active"
trap - ERR
sha256sum "$HOME/bin/iono-pfb" "$HOME/bin/iono-prn" "$HOME/bin/iono-verso" "$HOME/bin/iono-receiver"
printf 'Rollback: bash %q\n' "$state/rollback.sh"
