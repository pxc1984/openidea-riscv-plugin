#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
cargo build --locked --release --target wasm32-wasip2
mkdir -p dist
cp target/wasm32-wasip2/release/openidea_plugin_openidea_riscv.wasm dist/plugin.wasm
