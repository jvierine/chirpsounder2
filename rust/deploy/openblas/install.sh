#!/usr/bin/env bash
# Station-local, vendor-packaged OpenBLAS; does not replace system libraries.
set -euo pipefail
prefix="$HOME/.local/lib/iono-openblas"
mkdir -p "$prefix"
if [[ $# == 1 && "$1" == --existing ]]; then
    :
elif [[ $# == 1 ]]; then
    dpkg-deb -x "$1" "$prefix"
else
    (cd "$prefix" && apt-get download libopenblas0-pthread)
    for package in "$prefix"/*.deb; do dpkg-deb -x "$package" "$prefix"; done
fi
library=$(find "$prefix/usr/lib" -path '*/openblas-pthread/libopenblas.so.0' -print -quit)
[[ -n "$library" && -f "$library" ]]
unit="$HOME/.config/systemd/user/iono-prn@.service.d"
mkdir -p "$unit"
cat > "$unit/40-openblas.conf" <<EOF
[Service]
Environment=IONO_PRN_LAPACK=$library
Environment=OPENBLAS_NUM_THREADS=1
EOF
systemctl --user daemon-reload
printf 'OpenBLAS installed: %s\nPRN configuration ready; running workers were not restarted.\n' "$library"
