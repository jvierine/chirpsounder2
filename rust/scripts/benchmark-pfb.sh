#!/usr/bin/env bash
# Standalone Linux experiment, no station acquisition/service changes.
set -euo pipefail
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
output=${1:?provide an output binary path}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/src"
export CARGO_TARGET_DIR="${PFB_BENCH_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/chirpsounder2/pfb-benchmark}"
cat > "$work/Cargo.toml" <<'TOML'
[package]
name = "iono-pfb-bench"
version = "0.1.0"
edition = "2021"
[dependencies]
num-complex = "=0.4.6"
anyhow = "=1.0.104"
rustfft = "=6.4.1"
[profile.release]
lto = "thin"
codegen-units = 1
TOML
cp "$root/rust/benchmarks/pfb.rs" "$work/src/main.rs"
cp "$root/rust/crates/iono-core/src/simd.rs" "$work/src/simd.rs"
cp "$root/rust/crates/iono-core/src/pfb.rs" "$work/src/pfb.rs"
{
    echo 'use std::f64::consts::TAU;'
    sed -n '/^fn i0/,/^pub fn downconvert/{ /^pub fn downconvert/!p; }' "$root/rust/crates/iono-core/src/dsp.rs"
} > "$work/src/dsp.rs"
{
    echo 'use num_complex::Complex32 as Z; use anyhow::{ensure,Result}; use std::f64::consts::TAU;'
    echo 'use crate::{dsp::fir_taps,simd};'
    sed -n '/^pub struct Channelizer/,/^\/\/\/ FFT correlation/{ /^\/\/\/ FFT correlation/!p; }' "$root/rust/crates/iono-core/src/prn.rs"
} > "$work/src/prn.rs"
RUSTFLAGS='-C target-feature=+crt-static' CARGO_BUILD_JOBS=1 \
    cargo test --release --manifest-path "$work/Cargo.toml"
RUSTFLAGS='-C target-feature=+crt-static' CARGO_BUILD_JOBS=1 \
    cargo build --release --manifest-path "$work/Cargo.toml"
install -m755 "$CARGO_TARGET_DIR/release/iono-pfb-bench" "$output"
printf 'Built %s\n' "$output"
