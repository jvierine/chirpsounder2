#!/usr/bin/env bash
set -euo pipefail

SERVICE_NAME="chirpsounder_dombas.service"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE_SERVICE="${SCRIPT_DIR}/${SERVICE_NAME}"
USER_UNIT_DIR="${HOME}/.config/systemd/user"
TARGET_SERVICE="${USER_UNIT_DIR}/${SERVICE_NAME}"

ENABLE_LINGER=0

usage() {
    cat <<EOF
Install the Dombas chirpsounder service as a systemd user service on Ubuntu 24.04.

Usage:
  $(basename "$0") [--enable-linger]

Options:
  --enable-linger  Try to enable lingering so the user service can stay up after logout
                   and start at boot. This may prompt for sudo.
  -h, --help       Show this help text.
EOF
}

while (($#)); do
    case "$1" in
        --enable-linger)
            ENABLE_LINGER=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if [[ ! -f "${SOURCE_SERVICE}" ]]; then
    echo "Service template not found: ${SOURCE_SERVICE}" >&2
    exit 1
fi

mkdir -p "${USER_UNIT_DIR}"
cp "${SOURCE_SERVICE}" "${TARGET_SERVICE}"

systemctl --user daemon-reload
systemctl --user enable --now "${SERVICE_NAME}"

echo
echo "Installed user service:"
echo "  ${TARGET_SERVICE}"
echo
echo "Useful commands:"
echo "  systemctl --user status ${SERVICE_NAME}"
echo "  systemctl --user restart ${SERVICE_NAME}"
echo "  journalctl --user -u ${SERVICE_NAME} -f"

if (( ENABLE_LINGER )); then
    if command -v loginctl >/dev/null 2>&1; then
        echo
        echo "Enabling linger for user ${USER}..."
        sudo loginctl enable-linger "${USER}"
    else
        echo
        echo "loginctl not found, so lingering was not enabled." >&2
    fi
else
    echo
    echo "If you want the service to survive logout and start at boot, rerun with:"
    echo "  $(basename "$0") --enable-linger"
fi
