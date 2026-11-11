#!/usr/bin/env bash
# Balanced candidate ordering for a busy host. Run the parent shell with the
# desired nice/affinity settings; children inherit them. JSON markers preserve
# the outer round because each bounded benchmark invocation uses one trial.
set -euo pipefail
binary=${1:?provide the benchmark binary}
rounds=${PFB_CONFIRM_ROUNDS:-6}
IFS=, read -r -a sizes <<< "${PFB_CONFIRM_SIZES:-32,48,60,88,96,108}"
for ((round=0; round<rounds; round++)); do
    offset=$((round % ${#sizes[@]}))
    order=("${sizes[@]:offset}" "${sizes[@]:0:offset}")
    if (((round / ${#sizes[@]}) % 2)); then
        reverse=()
        for ((i=${#order[@]}-1; i>=0; i--)); do reverse+=("${order[i]}"); done
        order=("${reverse[@]}")
    fi
    joined=$(IFS=,; printf '%s' "${order[*]}")
    printf '{"test":"confirmation_round","round":%d,"sizes":"%s"}\n' "$round" "$joined"
    PFB_SIZES="$joined" PFB_TRIALS=1 "$binary"
done
