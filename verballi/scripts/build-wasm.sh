#!/usr/bin/env bash
# Build the WebAssembly package for browser / Electron.
# Requires: cargo install wasm-pack
set -euo pipefail
cd "$(dirname "$0")/.."

wasm-pack build crates/verballi-wasm --target web --release
echo "done: crates/verballi-wasm/pkg/"
echo "Optionally shrink: wasm-opt -Oz crates/verballi-wasm/pkg/verballi_wasm_bg.wasm -o ..."
echo "See docs/WEB.md for the AudioWorklet + ElevenLabs wiring."
