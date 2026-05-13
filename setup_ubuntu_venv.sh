#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENV_DIR="${VENV_DIR:-$SCRIPT_DIR/.venv}"
DIGITAL_RF_REPO="${DIGITAL_RF_REPO:-https://github.com/MITHaystack/digital_rf.git}"
DIGITAL_RF_SRC_DIR="${DIGITAL_RF_SRC_DIR:-$(mktemp -d)/digital_rf}"

echo "Installing Ubuntu system packages needed for chirpsounder2..."
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  cmake \
  gcc \
  g++ \
  git \
  libboost-all-dev \
  libfftw3-dev \
  libgeos-dev \
  libhdf5-dev \
  libopenmpi-dev \
  libproj-dev \
  libusb-1.0-0-dev \
  libuhd-dev \
  openmpi-bin \
  pkg-config \
  proj-bin \
  proj-data \
  gnuradio \
  python3-gi \
  python3 \
  python3-dev \
  python3-pip \
  python3-venv

if ! pkg-config --exists digital_rf; then
  echo "Installing Digital RF C library system-wide from source..."
  git clone --depth 1 "$DIGITAL_RF_REPO" "$DIGITAL_RF_SRC_DIR"
  cmake -S "$DIGITAL_RF_SRC_DIR/c" -B "$DIGITAL_RF_SRC_DIR/build-c"
  cmake --build "$DIGITAL_RF_SRC_DIR/build-c" -j"$(nproc)"
  sudo cmake --install "$DIGITAL_RF_SRC_DIR/build-c"
  sudo ldconfig
else
  echo "Digital RF C library already available via pkg-config; skipping source install."
fi

echo "Creating virtual environment at $VENV_DIR..."
python3 -m venv --system-site-packages "$VENV_DIR"

# shellcheck disable=SC1091
source "$VENV_DIR/bin/activate"

echo "Upgrading pip tooling..."
python -m pip install --upgrade pip setuptools wheel

echo "Installing Python dependencies from requirements.txt..."
python -m pip install -r "$SCRIPT_DIR/requirements.txt"

echo "Building local native components..."
make -C "$SCRIPT_DIR"

echo "Verifying Digital RF C headers and pkg-config metadata..."
pkg-config --cflags --libs digital_rf

cat <<EOF

Setup complete.

To start using the environment:
  source "$VENV_DIR/bin/activate"

Notes:
  - The core chirp-processing scripts should now work from the venv.
  - The venv uses --system-site-packages so Ubuntu-provided modules such as
    GNU Radio bindings are visible inside the virtual environment.
  - The Digital RF C library is installed system-wide so rx_uhd.cpp and
    rx_uhd_ext_gps.cpp can include <digital_rf.h> through pkg-config.

EOF
