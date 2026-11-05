#!/usr/bin/env bash
set -euo pipefail
test "$(id -u)" = 0
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
    uhd-host libuhd-dev libhdf5-dev libhdf5-openmpi-103-1t64 libfftw3-dev \
    build-essential cmake pkg-config git curl ca-certificates rsync \
    libboost-program-options-dev libboost-system-dev libboost-thread-dev \
    libboost-date-time-dev libboost-regex-dev libboost-serialization-dev
