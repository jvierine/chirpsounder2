#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
IONO_TARGET_DIR="${IONO_TARGET_DIR:-/var/www/html/iono}"
UPLOAD_TARGET_DIR="${UPLOAD_TARGET_DIR:-/var/www/html}"
DATA_TARGET_DIR="${DATA_TARGET_DIR:-/mnt/shovel/ionosonde}"
APACHE_CONF_DIR="${APACHE_CONF_DIR:-/etc/apache2/conf-available}"
APACHE_SERVICE="${APACHE_SERVICE:-apache2}"
APACHE_CONF_NAME="${APACHE_CONF_NAME:-upload-limit}"
WEB_WRITE_GROUP="${WEB_WRITE_GROUP:-www-data}"
SUDO="${SUDO:-sudo}"
FILES_TO_DEPLOY=(
    "${SCRIPT_DIR}/index.php:${IONO_TARGET_DIR}"
    "${SCRIPT_DIR}/favicon.ico:${IONO_TARGET_DIR}"
    "${SCRIPT_DIR}/favicon.svg:${IONO_TARGET_DIR}"
    "${SCRIPT_DIR}/uit-logo.png:${IONO_TARGET_DIR}"
    "${SCRIPT_DIR}/unis-logo-liggende.svg:${IONO_TARGET_DIR}"
    "${SCRIPT_DIR}/university-of-scranton-logo.png:${IONO_TARGET_DIR}"
    "${SCRIPT_DIR}/map_all.png:${IONO_TARGET_DIR}"
    "${SCRIPT_DIR}/map_scand.png:${IONO_TARGET_DIR}"
    "${SCRIPT_DIR}/map_us.png:${IONO_TARGET_DIR}"
    "${SCRIPT_DIR}/chirp_band_aoa_2026-05-20_000_020ms.png:${IONO_TARGET_DIR}"
    "${SCRIPT_DIR}/chirp_band_aoa_2026-05-20_020_040ms.png:${IONO_TARGET_DIR}"
    "${SCRIPT_DIR}/chirp_band_aoa_2026-05-20_040_060ms.png:${IONO_TARGET_DIR}"
    "${SCRIPT_DIR}/chirp_band_aoa_2026-05-20_120_140ms.png:${IONO_TARGET_DIR}"
    "${SCRIPT_DIR}/upload.php:${UPLOAD_TARGET_DIR}"
    "${SCRIPT_DIR}/upload_h5.php:${UPLOAD_TARGET_DIR}"
    "${SCRIPT_DIR}/upload-limit.conf:${APACHE_CONF_DIR}"
)
LEGACY_UPLOAD_FILES=(
    "${UPLOAD_TARGET_DIR}/upload_status.php"
    "${UPLOAD_TARGET_DIR}/upload-status.php"
)

for file_spec in "${FILES_TO_DEPLOY[@]}"; do
    source_file="${file_spec%%:*}"
    target_dir="${file_spec#*:}"
    if [[ ! -f "$source_file" ]]; then
        echo "Source file not found: $source_file" >&2
        exit 1
    fi

    $SUDO mkdir -p "$target_dir"
    target_file="${target_dir}/$(basename "$source_file")"
    echo "Deploying $source_file to $target_file"
    $SUDO install -m 0644 "$source_file" "$target_file"
done

for legacy_file in "${LEGACY_UPLOAD_FILES[@]}"; do
    echo "Removing legacy upload endpoint ${legacy_file}"
    $SUDO rm -f "$legacy_file"
done

echo "Making upload target directories writable by group ${WEB_WRITE_GROUP}"
$SUDO mkdir -p "$IONO_TARGET_DIR" "$DATA_TARGET_DIR"
$SUDO chgrp "$WEB_WRITE_GROUP" "$IONO_TARGET_DIR" "$DATA_TARGET_DIR"
$SUDO chmod 2775 "$IONO_TARGET_DIR" "$DATA_TARGET_DIR"

if command -v a2enconf >/dev/null 2>&1; then
    echo "Enabling Apache config $APACHE_CONF_NAME"
    $SUDO a2enconf "$APACHE_CONF_NAME" >/dev/null
fi

echo "Restarting Apache service $APACHE_SERVICE"
if command -v systemctl >/dev/null 2>&1; then
    $SUDO systemctl restart "$APACHE_SERVICE"
else
    $SUDO service "$APACHE_SERVICE" restart
fi

echo "Deployed locally to $IONO_TARGET_DIR, $UPLOAD_TARGET_DIR, and $APACHE_CONF_DIR"
