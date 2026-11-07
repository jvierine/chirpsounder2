#!/usr/bin/env bash
set -euo pipefail
root=${1:?};label=${2:?};out=/tmp/cc-origin-audit-$label
mkdir -p "$out"
for row in '5 1794075338' '7 1794075364' '9 1794075389' '11 1794075415'; do
 read -r mhz epoch <<< "$row"
 echo "$label ${mhz}MHz"
 nice -n15 /tmp/cc-pulse-audit "$root/digisonde_code_matches-$epoch.h5" "$((mhz*1000000))" "$out/$mhz.h5" | sed -n '1,2p'
done
