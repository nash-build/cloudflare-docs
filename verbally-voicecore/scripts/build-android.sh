#!/usr/bin/env bash
# Build verbally shared libs (.so) for Android ABIs using cargo-ndk.
# Requires: Android NDK, and:
#   cargo install cargo-ndk
#   rustup target add aarch64-linux-android armv7-linux-androideabi \
#                     x86_64-linux-android i686-linux-android
set -euo pipefail
cd "$(dirname "$0")/.."

OUT=dist/android/jniLibs
mkdir -p "$OUT"

echo "==> building cdylib for all Android ABIs"
cargo ndk \
  -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 \
  -o "$OUT" \
  build --release -p verbally-ffi

echo "done: $OUT"
echo "Copy jniLibs/ into your app module (src/main/jniLibs) and load with"
echo "System.loadLibrary(\"verbally_ffi\"); declare the externs via JNI."
