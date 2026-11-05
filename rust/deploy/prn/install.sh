#!/usr/bin/env bash
# Run as the receiver user, from a complete source snapshot. Never restart UHD.
set -euo pipefail
station=${1:?DOB, TGO or KHO}
receiver=${2:?path to built iono-prn}
plots=${3:?path to built iono-plots}
case "$station" in DOB|TGO|KHO) ;; *) exit 64;; esac
root=$(cd -- "$(dirname -- "$0")/../.." && pwd)
for binary in "$receiver" "$plots"; do
    test -x "$binary"
    if ldd "$binary" | grep -q 'not found'; then echo "Missing runtime libraries: $binary" >&2; exit 1; fi
done
test -w /data0
mkdir -p "$HOME/bin" "$HOME/.config/iono-prn" "$HOME/.config/systemd/user"
install -m755 "$receiver" "$HOME/bin/iono-prn.next"
mv "$HOME/bin/iono-prn.next" "$HOME/bin/iono-prn"
install -m755 "$plots" "$HOME/bin/iono-plots.next"
mv "$HOME/bin/iono-plots.next" "$HOME/bin/iono-plots"
install -m644 "$root/deploy/iono-prn@.service" "$root/deploy/iono-prn.slice" "$HOME/.config/systemd/user/"
sources=(Kiruna)
[[ "$station" != DOB ]] || sources+=(Uppsala)
for source in "${sources[@]}"; do
    install -m644 "$root/deploy/prn/$station-$source.json" "$HOME/.config/iono-prn/$source.json"
done
if [[ "$station" == TGO ]]; then
    browser=iono-local-plots
    config="$HOME/src/chirpsounder2/rust/deploy/tgo-rust.json"
    flags='--browser-only'
else
    browser=iono-browser-arrays
    config="$HOME/.config/iono-browser.json"
    flags='--browser-only'
    [[ "$station" != DOB ]] || flags+=' --no-upload'
fi
test -f "$config"
mkdir -p "$HOME/.local/state/iono-prn-rollout" "$HOME/.config/systemd/user/$browser.service.d"
backup="$HOME/.local/state/iono-prn-rollout/browser-before.conf"
[[ -e "$backup" ]] || systemctl --user cat "$browser" > "$backup"
# Order after DOB's existing transport.conf; retain its no-upload policy above.
rm -f "$HOME/.config/systemd/user/$browser.service.d/prn.conf"
cat > "$HOME/.config/systemd/user/$browser.service.d/zz-prn.conf" <<EOF
[Service]
ExecStart=
ExecStart=%h/bin/iono-plots $config $flags
EOF
systemctl --user daemon-reload
systemctl --user restart "$browser"
for source in "${sources[@]}"; do
    systemctl --user enable "iono-prn@$source"
    systemctl --user restart "iono-prn@$source"
done
systemctl --user --no-pager status "$browser" "iono-prn@Kiruna"
