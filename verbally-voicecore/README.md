# voicecore

Real-time, single-microphone **near-field voice isolation** for voice agents.
It keeps the *enrolled* speaker (you), attenuates other voices and ambient
noise, and emits **16 kHz mono PCM** ready to stream straight into an
**ElevenLabs** speech-to-speech / Conversational-AI agent.

One shared **Rust** engine compiles to **PC, macOS, iOS, and Android** (plus
WebAssembly), with thin native shells for mic capture and the ElevenLabs
connection.

> **Clean-room.** This is built from first principles using open DSP/ML
> techniques and permissively-licensed building blocks. It contains **no
> third-party SDK code** — the ElevenLabs connection uses ElevenLabs' own
> published SDK/protocol in the platform shell, unchanged. See
> [`docs/CLEANROOM.md`](docs/CLEANROOM.md).

## Why "near-field"

A single mic can't measure distance geometrically, so "focus on what's closest
to the mic" becomes a *learned* problem, exactly like Krisp / NVIDIA Broadcast:

1. **Enroll** ~20–30 s of your voice → a compact speaker fingerprint.
2. At runtime a **target-speaker gate** keeps audio matching your fingerprint and
   attenuates everyone else, helped by a **proximity** estimator (spectral tilt
   + direct-to-reverberant cues) and a **denoiser** for ambient noise.

## Layout

```
crates/voicecore            the engine (DSP + speaker extraction), no I/O, fully tested
crates/voicecore-ffi        C ABI (+ header) native shells link against
crates/voicecore-deepfilter DeepFilterNet ERB enhancer (open deep_filter crate)
crates/voicecore-onnx       ONNX adapters: ECAPA verifier, ONNX enhancer, log-mel front-end
crates/voicecore-wasm       WebAssembly bindings + web kit (browser / Electron / Lovable)
apps/cli                    reference CLI: enroll, process files, live streaming
scripts/                    per-platform build scripts (Apple, Android, desktop, wasm)
docs/                       architecture, deployment, web, ElevenLabs, neural, clean-room
```

## Quick start

```bash
# Build + test the portable engine
cargo test

# Reference CLI
cargo build --release -p voicecore-cli

# 1) Enroll your voice (20–30 s of just you, any-rate WAV)
./target/release/voicecore enroll you.wav you.profile

# 2) Isolate a file
./target/release/voicecore process noisy.wav clean.wav --profile you.profile

# 3) Live: cleaned mic → your ElevenLabs agent (needs ALSA headers on Linux)
cargo build --release -p voicecore-cli --features live
export ELEVENLABS_API_KEY=...        # only for private agents
./target/release/voicecore live --agent-id <AGENT_ID> --profile you.profile
```

## Using the engine from Rust

```rust
use voicecore::{Config, VoiceEngine};

let mut cfg = Config::default();
cfg.input_sample_rate = 48_000;          // your mic's rate
let mut engine = VoiceEngine::new(cfg)?;

engine.begin_enrollment();
let _ = engine.process(&your_voice_samples);     // ~20–30 s
let profile = engine.finish_enrollment().unwrap();
std::fs::write("you.profile", profile.to_bytes())?;

// Per audio chunk from the mic:
let clean_16k_mono = engine.process(&mic_chunk); // → ElevenLabs
```

## Tuning ([`Config`])

| Field | Default | Meaning |
|-------|---------|---------|
| `denoise_strength` | 0.75 | ambient-noise suppression, 0..1 |
| `speaker_focus` | 0.80 | how hard to reject non-enrolled voices, 0..1 |
| `proximity_focus` | 0.40 | weight of the near-field/distance cue, 0..1 |
| `speaker_gate_floor` | 0.05 | never fully mute (keeps EL turn-taking happy) |
| `highpass_hz` | 80 | rumble/handling-noise cut |
| `agc_target_dbfs` | -18 | output leveling target |

## Performance

The pure-DSP path runs at ~**0.001× real time** on a laptop core (measured via
`voicecore bench`), leaving ample budget to drop in a neural denoiser
(DeepFilterNet) or ECAPA speaker model via the provided seams.

## Upgrading to SOTA (open models)

The default build uses open DSP and an **MFCC** speaker fingerprint. Typed seams
let you drop in the open trained models exactly where they belong, with no
proprietary code:

| Capability | Seam | Open model | Status |
|------------|------|------------|--------|
| Neural denoise | `voicecore-deepfilter` (`Enhancer`) | **DeepFilterNet** (Rust, MIT/Apache) | ERB enhancer runnable now; trained-weights slot via `BandGainModel` |
| Per-bin neural mask | `dsp::denoise::NeuralDenoiser` | DTLN / custom | seam ready |
| Window-level speaker lock | `speaker::verify::PresenceScorer` + `SpeakerVerifier` | **ECAPA-TDNN** (ONNX) | rolling re-score runnable now (MFCC); ECAPA adapter shipped in `voicecore-onnx` |
| ONNX runtime adapters | `voicecore-onnx` (`OnnxEcapaVerifier`, `OnnxEnhancer`, `MelFrontend`) | any ONNX export | compiled against `ort`; dynamic-loaded runtime, bring a model |

```bash
# DeepFilterNet ERB enhancer through the CLI:
voicecore process noisy.wav clean.wav --deepfilter 0.9 --denoise 0.2
```

- **How it all works** (public-knowledge brief): [`docs/HOW_IT_WORKS.md`](docs/HOW_IT_WORKS.md)
- **Concrete model wiring**: [`docs/NEURAL_MODELS.md`](docs/NEURAL_MODELS.md)
- **Architecture**: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)

## Deployment

Per-platform packaging (Apple XCFramework, Android `jniLibs`, desktop libs) is in
[`docs/DEPLOYMENT.md`](docs/DEPLOYMENT.md). ElevenLabs wiring is in
[`docs/ELEVENLABS.md`](docs/ELEVENLABS.md). For a browser/React SaaS app
(e.g. **Lovable**), the drop-in web kit and steps are in
[`docs/LOVABLE.md`](docs/LOVABLE.md) (`crates/voicecore-wasm/web/`).

## License

Dual-licensed under MIT or Apache-2.0.
