#!/usr/bin/env bash
# Pin the Rustplotlib implementation shared by all live browser plots.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
revision=$(rg -o 'pkg/[a-f0-9]{40}/rustplotlib_web\.js' web/app.js | cut -d/ -f2)
[[ "$revision" =~ ^[a-f0-9]{40}$ ]] || { echo 'Missing Rustplotlib revision in web/app.js'; exit 1; }
source_dir=${RUSTPLOTLIB_SOURCE:-"$(pwd)/.deps/rustplotlib"}
if [[ ! -d "$source_dir/.git" ]]; then
    git clone https://github.com/jvierine/rustplotlib "$source_dir"
fi
if [[ "$(git -C "$source_dir" rev-parse HEAD)" != "$revision" ]]; then
    git -C "$source_dir" fetch origin "$revision"
    git -C "$source_dir" checkout --detach "$revision"
fi
rustup target add wasm32-unknown-unknown
cargo build --manifest-path "$source_dir/viewer/Cargo.toml" --release --target wasm32-unknown-unknown --locked
wasm-bindgen --version | rg -q '0\.2\.129$' || { echo 'Install wasm-bindgen-cli 0.2.129 to match Rustplotlib'; exit 1; }
wasm-bindgen "$source_dir/target/wasm32-unknown-unknown/release/rustplotlib_web.wasm" --target web --out-dir "web/pkg/$revision"
cp "$source_dir/viewer/js/navigation.js" web/navigation.js
