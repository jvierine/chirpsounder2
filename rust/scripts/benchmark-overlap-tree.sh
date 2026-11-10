#!/usr/bin/env bash
# Standalone Linux experiment, no station acquisition/service changes.
set -euo pipefail
root=$(git -C "$(dirname "$0")" rev-parse --show-toplevel)
output=${1:?provide an output binary path}
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/src"
cat > "$work/Cargo.toml" <<'TOML'
[package]
name = "iono-overlap-tree-bench"
version = "0.1.0"
edition = "2021"
[dependencies]
num-complex = "=0.4.6"
anyhow = "=1.0.104"
[profile.release]
lto = "thin"
codegen-units = 1
TOML
cp "$root/rust/benchmarks/overlap-tree.rs" "$work/src/main.rs"
cp "$root/rust/crates/iono-core/src/simd.rs" "$work/src/simd.rs"
cp "$root/rust/crates/iono-core/src/channel_tree.rs" "$work/src/channel_tree.rs"
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
install -m755 "$work/target/release/iono-overlap-tree-bench" "$output"
printf 'Built %s\n' "$output"
