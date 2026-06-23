#!/usr/bin/env bash
# Build the ONNX adapters (ECAPA verifier + ONNX enhancer).
# ONNX Runtime is dynamically loaded — no build-time download. At runtime, set
# ORT_DYLIB_PATH to your libonnxruntime (or place it beside the binary).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -p verbally-onnx --features ort
echo "done. Run with: ORT_DYLIB_PATH=/path/to/libonnxruntime.so ./your-binary"
echo "See docs/NEURAL_MODELS.md for ECAPA / enhancer wiring."
