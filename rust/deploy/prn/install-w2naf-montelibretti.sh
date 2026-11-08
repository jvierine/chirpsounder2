#!/usr/bin/env bash
# Install receiver-local PRN reception; preserve UHD and existing sounders.
set -euo pipefail
receiver=${1:?path to built iono-prn}
delay=${2:?path to built iono-delay}
root=$(cd -- "$(dirname -- "$0")/../.." && pwd)
config="$root/deploy/stations/W2NAF/iono-prn/Montelibretti.json"
for binary in "$receiver" "$delay"; do
  test -x "$binary"
  if ldd "$binary" | grep -q 'not found'; then echo "Missing libraries: $binary" >&2; exit 1; fi
done
mkdir -p "$HOME/bin" "$HOME/.config/iono-prn" "$HOME/.config/systemd/user/iono-prn@Montelibretti.service.d"
install -m755 "$receiver" "$HOME/bin/iono-prn-w2naf-montelibretti"
install -m755 "$delay" "$HOME/bin/iono-delay"
install -m644 "$config" "$HOME/.config/iono-prn/Montelibretti.json"
install -m644 "$root/deploy/iono-prn@.service" "$root/deploy/iono-prn.slice" "$HOME/.config/systemd/user/"
cat > "$HOME/.config/systemd/user/iono-prn@Montelibretti.service.d/native.conf" <<'UNIT'
[Service]
ExecStart=
ExecStart=%h/bin/iono-prn-w2naf-montelibretti %h/.config/iono-prn/Montelibretti.json
Environment=OPENBLAS_NUM_THREADS=1
Environment=OMP_NUM_THREADS=1
Environment=IONO_PRN_LAPACK=/usr/lib/x86_64-linux-gnu/libopenblas.so
UNIT
systemctl --user daemon-reload
systemctl --user enable --now iono-prn@Montelibretti.service
systemctl --user --no-pager status iono-prn@Montelibretti.service
