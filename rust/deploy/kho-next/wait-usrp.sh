#!/usr/bin/env bash
set -euo pipefail
chronyc waitsync 60 0.1
test -w /data0
[[ $(df --output=avail -k /data0 | tail -1) -gt 10485760 ]]
until ping -n -c1 -W1 192.168.10.2 >/dev/null; do sleep 2; done
# This probe runs before acquisition, never alongside a recording process.
"$HOME/bin/iono-usrp-status" addr=192.168.10.2 /data0/.station-usrp.json
jq -e '.hardware.mboard_serial == "E9R17S3UP"' /data0/.station-usrp.json >/dev/null
