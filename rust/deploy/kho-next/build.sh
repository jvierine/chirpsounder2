#!/bin/bash
set -euo pipefail
cd "$HOME/src/digital_rf"
cmake -S c -B build-c -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$HOME/.local" -DCMAKE_INSTALL_LIBDIR=lib
cmake --build build-c -j6
cmake --install build-c
export PKG_CONFIG_PATH="$HOME/.local/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
export LD_LIBRARY_PATH="$HOME/.local/lib:${LD_LIBRARY_PATH:-}"
cd "$HOME/src/chirpsounder2"
make -j2 rx_uhd_ext_gps
g++ -O2 rust/deploy/krea/usrp-status.cpp -o "$HOME/bin/iono-usrp-status" $(pkg-config --cflags --libs uhd hdf5)
install -m755 rx_uhd_ext_gps "$HOME/bin/"
cd rust
export HDF5_DIR=/usr/lib/x86_64-linux-gnu/hdf5/serial
"$HOME/.cargo/bin/cargo" build --release --locked -j6 -p iono-receiver -p iono-plots
for name in iono-receiver iono-prn iono-verso iono-delay iono-plots; do
 install -m755 "target/release/$name" "$HOME/bin/$name"
done
"$HOME/.cargo/bin/cargo" test --release --locked -j6 -p iono-core --no-default-features -p iono-receiver --bins --lib
printf '\nBUILD_AND_TEST_OK\n'
