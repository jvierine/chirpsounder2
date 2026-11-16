#!/usr/bin/env bash
# DOB-only retirement: retain configuration and historical products.
set -euo pipefail
[[ $(hostname -s) == tgo-mic1 ]] || exit 64
systemctl --user disable --now iono-prn@Huairou.service
report="${XDG_STATE_HOME:-$HOME/.local/state}/iono-processing/prn-Huairou.json"
if [[ -f "$report" ]]; then
    mkdir -p "${report%/*}/retired"
    mv "$report" "${report%/*}/retired/prn-Huairou-$(date -u +%Y%m%dT%H%M%S).json"
fi
systemctl --user show iono-prn@Huairou.service -p ActiveState -p MainPID -p UnitFileState
[[ $(systemctl --user show iono-prn@Huairou.service -p MainPID --value) == 0 ]]
[[ $(systemctl --user is-enabled iono-prn@Huairou.service || true) == disabled ]]
