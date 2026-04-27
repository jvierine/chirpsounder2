#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE_FILE="${SOURCE_FILE:-$SCRIPT_DIR/index.php}"
TARGET_DIR="${TARGET_DIR:-/var/www/html/iono}"
TARGET_FILE="${TARGET_FILE:-$TARGET_DIR/index.php}"

if [[ ! -f "$SOURCE_FILE" ]]; then
    echo "Source file not found: $SOURCE_FILE" >&2
    exit 1
fi

mkdir -p "$TARGET_DIR"

echo "Deploying $SOURCE_FILE to $TARGET_FILE"
install -m 0644 "$SOURCE_FILE" "$TARGET_FILE"
echo "Deployed locally to $TARGET_FILE"
