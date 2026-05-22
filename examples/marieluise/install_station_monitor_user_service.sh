#!/usr/bin/env bash
set -euo pipefail

SERVICE_NAME="${SERVICE_NAME:-chirpsounder_station_monitor.service}"
REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONFIG_FILE="${REPO_DIR}/examples/marieluise/tgo.ini"
PYTHON_BIN="${PYTHON_BIN:-$(command -v python3)}"
UPLOAD_URL="${UPLOAD_URL:-http://4.235.86.214/upload.php}"
PERIOD_S="${PERIOD_S:-900}"
ENABLE_LINGER=0

usage() {
    cat <<EOF
Install the chirpsounder station monitor as a systemd user service.

Usage:
  $(basename "$0") [--config PATH] [--repo PATH] [--python PATH] [--upload-url URL] [--period-s SECONDS] [--enable-linger]

Options:
  --config PATH      Chirpsounder config file. Default: ${CONFIG_FILE}
  --repo PATH        chirpsounder2 repository path. Default: ${REPO_DIR}
  --python PATH      Python interpreter. Default: ${PYTHON_BIN}
  --upload-url URL   Status JSON upload endpoint. Default: ${UPLOAD_URL}
  --period-s SEC     Monitoring interval. Default: ${PERIOD_S}
  --enable-linger    Try to keep the user service running after logout and at boot.
  -h, --help         Show this help text.
EOF
}

while (($#)); do
    case "$1" in
        --config)
            CONFIG_FILE="$2"
            shift 2
            ;;
        --repo)
            REPO_DIR="$2"
            shift 2
            ;;
        --python)
            PYTHON_BIN="$2"
            shift 2
            ;;
        --upload-url)
            UPLOAD_URL="$2"
            shift 2
            ;;
        --period-s)
            PERIOD_S="$2"
            shift 2
            ;;
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

if [[ ! -d "${REPO_DIR}" ]]; then
    echo "Repository directory not found: ${REPO_DIR}" >&2
    exit 1
fi

if [[ ! -f "${CONFIG_FILE}" ]]; then
    echo "Config file not found: ${CONFIG_FILE}" >&2
    exit 1
fi

if [[ ! -x "${PYTHON_BIN}" ]]; then
    echo "Python interpreter not executable: ${PYTHON_BIN}" >&2
    exit 1
fi

USER_UNIT_DIR="${HOME}/.config/systemd/user"
TARGET_SERVICE="${USER_UNIT_DIR}/${SERVICE_NAME}"
mkdir -p "${USER_UNIT_DIR}"

cat > "${TARGET_SERVICE}" <<EOF
[Unit]
Description=Chirpsounder station monitor
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
WorkingDirectory=${REPO_DIR}
ExecStart=${PYTHON_BIN} ${REPO_DIR}/station_monitor.py --config ${CONFIG_FILE} --upload-url ${UPLOAD_URL} --period-s ${PERIOD_S}
Restart=always
RestartSec=10

[Install]
WantedBy=default.target
EOF

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
    echo "If you want the monitor to survive logout and start at boot, rerun with:"
    echo "  $(basename "$0") --enable-linger"
fi
