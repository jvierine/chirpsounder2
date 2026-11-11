#!/bin/bash
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends build-essential cmake pkg-config git curl ca-certificates rsync \
 uhd-host libuhd-dev libhdf5-dev libopenblas-dev libfftw3-dev libfontconfig1-dev libfreetype6-dev \
 libboost-program-options-dev libboost-system-dev libboost-thread-dev libboost-date-time-dev libboost-regex-dev libboost-serialization-dev \
 chrony ethtool iputils-ping jq
install -d -o administrator -g administrator /data0
loginctl enable-linger administrator
