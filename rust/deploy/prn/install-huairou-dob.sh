#!/usr/bin/env bash
# Add only the new DOB worker. Existing PRN, Digisonde, LFM and acquisition
# services keep their existing binaries and settings.
set -euo pipefail
binary=${1:?path to tested native-rate iono-prn binary}
root=$(cd "$(dirname "$0")/../.." && pwd)
test -x "$binary"
if ldd "$binary" | grep -q 'not found'; then echo 'Missing receiver libraries' >&2; exit 1; fi
test -f "$HOME/.config/systemd/user/iono-prn@.service"
mkdir -p "$HOME/bin" "$HOME/.config/iono-prn" "$HOME/.config/systemd/user/iono-prn@Huairou.service.d"
install -m755 "$binary" "$HOME/bin/iono-prn-huairou.next"
mv -f "$HOME/bin/iono-prn-huairou.next" "$HOME/bin/iono-prn-huairou"
install -m600 "$root/deploy/prn/DOB-Huairou.json" "$HOME/.config/iono-prn/Huairou.json"
cat > "$HOME/.config/systemd/user/iono-prn@Huairou.service.d/native-code-clock.conf" <<'UNIT'
[Service]
ExecStart=
ExecStart=%h/bin/iono-prn-huairou %h/.config/iono-prn/Huairou.json
Environment=OPENBLAS_NUM_THREADS=1
UNIT
systemctl --user daemon-reload
systemctl --user enable iono-prn@Huairou.service
systemctl --user restart iono-prn@Huairou.service
systemctl --user is-active iono-prn@Huairou.service
