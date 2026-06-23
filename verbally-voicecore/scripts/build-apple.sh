#!/usr/bin/env bash
# Build verbally as an XCFramework for macOS + iOS (device & simulator).
# Requires: macOS, Xcode, and the Rust Apple targets:
#   rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios \
#                     aarch64-apple-darwin x86_64-apple-darwin
set -euo pipefail
cd "$(dirname "$0")/.."

CRATE=verbally-ffi
LIB=libverbally_ffi.a
OUT=dist/apple
mkdir -p "$OUT"

echo "==> building static libs"
for T in aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios \
         aarch64-apple-darwin x86_64-apple-darwin; do
  cargo build --release -p "$CRATE" --target "$T"
done

echo "==> fat libs (sim + macOS)"
mkdir -p "$OUT/ios-sim" "$OUT/macos"
lipo -create \
  target/aarch64-apple-ios-sim/release/$LIB \
  target/x86_64-apple-ios/release/$LIB \
  -output "$OUT/ios-sim/$LIB"
lipo -create \
  target/aarch64-apple-darwin/release/$LIB \
  target/x86_64-apple-darwin/release/$LIB \
  -output "$OUT/macos/$LIB"

echo "==> assembling XCFramework"
HDR=crates/verbally-ffi/include
rm -rf "$OUT/VoiceCore.xcframework"
xcodebuild -create-xcframework \
  -library target/aarch64-apple-ios/release/$LIB -headers "$HDR" \
  -library "$OUT/ios-sim/$LIB" -headers "$HDR" \
  -library "$OUT/macos/$LIB" -headers "$HDR" \
  -output "$OUT/VoiceCore.xcframework"

echo "done: $OUT/VoiceCore.xcframework"
echo "Add it to your Xcode target and import via the bridging header (verbally.h)."
