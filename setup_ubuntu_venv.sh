#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VENV_DIR="${VENV_DIR:-$SCRIPT_DIR/.venv}"

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

cat <<EOF

Setup complete.

To start using the environment:
  source "$VENV_DIR/bin/activate"

Notes:
  - The core chirp-processing scripts should now work from the venv.
  - The venv uses --system-site-packages so Ubuntu-provided modules such as
    GNU Radio bindings are visible inside the virtual environment.
  - If make fails on a fresh machine, install/build the Digital RF C/C++
    development package as well, because rx_uhd*.cpp links against digital_rf.

EOF
