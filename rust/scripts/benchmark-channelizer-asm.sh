#!/usr/bin/env bash
# Build a portable, Linux-only paired benchmark; no acquisition or station units.
# HDF5 is deliberately excluded so the same binary can run on older receivers.
set -euo pipefail
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
baseline=${BASELINE_REVISION:-3a1e3d0}
output=${1:?provide an output binary path}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/src"
cat > "$work/Cargo.toml" <<'TOML'
[package]
name = "iono-channelizer-asm-bench"
version = "0.1.0"
edition = "2021"
[dependencies]
num-complex = "=0.4.6"
anyhow = "=1.0.104"
[profile.release]
lto = "thin"
codegen-units = 1
TOML
if [[ -n "${BASELINE_SOURCE_DIR:-}" ]]; then
    # Allows a minimal source-only build host without transferring repository
    # history. Caller must export these exact files from BASELINE_REVISION.
    cp "$BASELINE_SOURCE_DIR/simd.rs" "$work/src/simd_before.rs"
    cp "$BASELINE_SOURCE_DIR/prn.rs" "$work/prn_before.rs"
else
    git -C "$root" show "$baseline:rust/crates/iono-core/src/simd.rs" > "$work/src/simd_before.rs"
    git -C "$root" show "$baseline:rust/crates/iono-core/src/prn.rs" > "$work/prn_before.rs"
fi
cp "$root/rust/crates/iono-core/src/simd.rs" "$work/src/simd_after.rs"
cp "$root/rust/benchmarks/channelizer-asm.rs" "$work/src/main.rs"
{
    echo 'use std::f64::consts::TAU;'
    sed -n '/^fn i0/,/^pub fn downconvert/{ /^pub fn downconvert/!p; }' "$root/rust/crates/iono-core/src/dsp.rs"
} > "$work/src/dsp.rs"
for version in before after; do
    source_prn="$root/rust/crates/iono-core/src/prn.rs"
    if [[ "$version" == before ]]; then source_prn="$work/prn_before.rs"; fi
    {
        echo 'use num_complex::Complex32 as Z; use anyhow::{ensure,Result}; use std::f64::consts::TAU;'
        echo "use crate::{dsp::fir_taps,simd_$version as simd};"
        sed -n '/^pub struct Channelizer/,/^\/\/\/ FFT correlation/{ /^\/\/\/ FFT correlation/!p; }' "$source_prn"
    } > "$work/src/channelizer_$version.rs"
done
# Use a baseline-compatible ISA; feature dispatch remains in the real kernels.
# Static glibc avoids coupling this timing tool to the build host's newer glibc.
RUSTFLAGS='-C target-feature=+crt-static' CARGO_BUILD_JOBS=1 \
    cargo build --release --manifest-path "$work/Cargo.toml"
install -m755 "$work/target/release/iono-channelizer-asm-bench" "$output"
printf 'Built %s against baseline %s\n' "$output" "$baseline"
