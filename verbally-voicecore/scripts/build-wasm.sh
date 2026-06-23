#!/usr/bin/env bash
# Build the WebAssembly package for browser / Electron.
# Requires: cargo install wasm-pack
set -euo pipefail
cd "$(dirname "$0")/.."

wasm-pack build crates/verbally-wasm --target web --release
echo "done: crates/verbally-wasm/pkg/"
echo "Optionally shrink: wasm-opt -Oz crates/verbally-wasm/pkg/verbally_wasm_bg.wasm -o ..."
echo "See docs/WEB.md for the AudioWorklet + ElevenLabs wiring."
