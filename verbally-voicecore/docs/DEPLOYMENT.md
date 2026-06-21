# Deployment

The engine is one Rust crate (`voicecore`) exposed through a C ABI
(`voicecore-ffi`). Each platform links the compiled library and provides the mic
capture + ElevenLabs connection in its native shell.

## Prerequisites

```bash
# Rust
curl https://sh.rustup.rs -sSf | sh

# Per-target toolchains (install what you ship)
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios \
                  aarch64-apple-darwin x86_64-apple-darwin
rustup target add aarch64-linux-android armv7-linux-androideabi \
                  x86_64-linux-android i686-linux-android
cargo install cargo-ndk          # Android
```

## macOS & iOS

```bash
./scripts/build-apple.sh         # → dist/apple/VoiceCore.xcframework
```

1. Drag `VoiceCore.xcframework` into your Xcode target.
2. Add `crates/voicecore-ffi/include/voicecore.h` to your bridging header.
3. Capture with **AVAudioEngine** (`installTap` on the input node), pass the
   buffer's `floatChannelData` to `vc_engine_process`, send the cleaned samples
   to ElevenLabs with their iOS SDK.

```swift
let engine = vc_engine_new(UInt32(format.sampleRate))
// enrollment once:
vc_engine_begin_enrollment(engine)
// per tap buffer:
var out = [Float](repeating: 0, count: frameCount)
var written = 0
vc_engine_process(engine, micPtr, frameCount, &out, out.count, &written)
// → feed out[0..<written] to the ElevenLabs stream
```

## Android

```bash
./scripts/build-android.sh       # → dist/android/jniLibs/<abi>/libvoicecore_ffi.so
```

1. Copy `jniLibs/` into `app/src/main/jniLibs/`.
2. Capture with **AudioRecord** (or Oboe), call the engine over JNI, send cleaned
   audio with the ElevenLabs Android/Kotlin SDK.

```kotlin
companion object { init { System.loadLibrary("voicecore_ffi") } }
external fun vcProcess(handle: Long, input: FloatArray, out: FloatArray): Int
```

> Tip: for the cleanest Swift/Kotlin bindings, add **UniFFI** to
> `voicecore-ffi` — it generates idiomatic bindings from the Rust API and removes
> hand-written JNI/bridging glue. The C ABI shipped here works without it.

## Windows / Linux desktop

```bash
./scripts/build-desktop.sh       # engine libs + reference CLI (with live mode)
```

The reference CLI (`apps/cli`) already implements capture (cpal) + ElevenLabs
streaming under `--features live`; use it as the template for an Electron/Tauri
or native desktop app. On Linux install `libasound2-dev` for the `live` feature.

## WebAssembly (browser / Electron)

Add `wasm-bindgen` to a thin wrapper crate and build with
`wasm-pack build --target web`. Capture via an **AudioWorklet**, call
`engine.process()` on each 128-sample render quantum, and send cleaned PCM to the
ElevenLabs JS SDK. (The core has no OS dependencies, so it compiles to
`wasm32-unknown-unknown` as-is.)

## What's left to you (the production gates)

These need credentials/accounts only you hold:

- **ElevenLabs**: your API key (private agents) and `agent_id`.
- **Apple**: a Developer account + signing to ship the iOS/macOS app.
- **Google Play**: a developer account + signing for the Android app.

Everything up to those gates — engine, bindings, build artifacts, reference
streaming — is in this repo.
