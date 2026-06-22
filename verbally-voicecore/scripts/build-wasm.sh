#!/usr/bin/env bash
# Build the WebAssembly package for browser / Electron.
# Requires: cargo install wasm-pack
set -euo pipefail
cd "$(dirname "$0")/.."

wasm-pack build crates/voicecore-wasm --target web --release
echo "done: crates/voicecore-wasm/pkg/"
echo "Optionally shrink: wasm-opt -Oz crates/voicecore-wasm/pkg/voicecore_wasm_bg.wasm -o ..."
echo "See docs/WEB.md for the AudioWorklet + ElevenLabs wiring."
