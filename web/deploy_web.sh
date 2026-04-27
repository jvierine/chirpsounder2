#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_DIR="${TARGET_DIR:-/var/www/html/iono}"
FILES_TO_DEPLOY=(
    "${SCRIPT_DIR}/index.php"
    "${SCRIPT_DIR}/upload_h5.php"
)

mkdir -p "$TARGET_DIR"

for source_file in "${FILES_TO_DEPLOY[@]}"; do
    if [[ ! -f "$source_file" ]]; then
        echo "Source file not found: $source_file" >&2
        exit 1
    fi

    target_file="${TARGET_DIR}/$(basename "$source_file")"
    echo "Deploying $source_file to $target_file"
    install -m 0644 "$source_file" "$target_file"
done

echo "Deployed locally to $TARGET_DIR"
