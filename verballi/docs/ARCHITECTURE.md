# Architecture

## Signal flow

```
mic ─▶ resample(→16 kHz) ─▶ high-pass ─▶ STFT ─┬─ VAD ───────────────────┐
                                                ├─ denoise mask (per-bin) ─┤ ×
                                                └─ speaker gate (scalar) ───┘
                                                                            │
                                              ISTFT (overlap-add) ◀─────────┘
                                                     │
                                              AGC + limiter ─▶ 16 kHz mono ─▶ ElevenLabs
```

Everything is streaming and runs on the audio thread's cadence; you can push
arbitrary chunk sizes in and pull whatever comes out.

## Modules (`crates/verballi`)

| Module | Role |
|--------|------|
| `resample` | streaming linear resampler (+ anti-alias pre-filter) to the fixed 16 kHz working rate |
| `dsp::biquad` | RBJ high-pass to kill DC / rumble / handling noise |
| `dsp::stft` | sqrt-Hann WOLA analysis/synthesis (512-pt, 50% hop = 32 ms / 16 ms) |
| `dsp::vad` | energy-vs-adaptive-floor + spectral-flatness voice activity, with hangover |
| `dsp::denoise` | decision-directed Wiener suppressor over a VAD-gated noise estimate |
| `dsp::agc` | RMS-tracking auto-gain toward a target dBFS + soft brick-wall limiter |
| `speaker::embedding` | log band-energy voice fingerprint (cepstral-mean-subtracted, L2-normalized) |
| `speaker::proximity` | single-mic "closeness" from spectral tilt + envelope definition |
| `speaker::gate` | combines identity similarity + proximity into a smoothed per-frame gain |
| `pipeline` | wires the graph; `lib::VoiceEngine` is the public façade |

## Why these choices

- **16 kHz mono** is what ElevenLabs STS/agents consume; doing all internal work
  there avoids redundant conversions.
- **sqrt-Hann at 50% overlap** satisfies the constant-overlap-add condition, so
  the round-trip is transparent at unity gain and per-bin masking is
  artifact-light (no "musical noise" from a rectangular window).
- **Target-speaker gate, not just a denoiser** — the requirement is to reject
  *other voices*, which generic noise suppression won't do. The gate keys off
  your enrolled fingerprint.
- **Gate floor** never fully mutes; momentary mismatches stay audible enough that
  ElevenLabs' own VAD / turn-taking doesn't misfire.

## Latency

Algorithmic latency is one STFT frame (512 samples @ 16 kHz ≈ **32 ms**) plus the
hop. That sits within a natural conversational budget. Drop the FFT size to 256
in `config.rs` to halve it at some cost to low-frequency resolution.

## Extension seams (drop-in neural models)

The DSP path is the always-available baseline. Two traits let you add
state-of-the-art models without touching the plumbing:

- **`dsp::denoise::NeuralDenoiser`** — return a per-bin gain mask. Wrap
  **DeepFilterNet** (Rust, MIT/Apache) here; it composes with the built-in
  Wiener mask. Install via `Pipeline::set_neural_denoiser`.
- **`speaker::embedding::Embedder`** — return a fixed-dim embedding per frame.
  Wrap **ECAPA-TDNN** exported to ONNX and run with the `ort` crate
  (CoreML execution provider on Apple, NNAPI on Android) for much stronger
  separation between similar human voices. Install via `Pipeline::set_embedder`.

Because both are `trait`s behind the same `Pipeline`, you can ship the DSP build
today and upgrade per platform as you validate the heavier models against the
~32 ms / real-time budget.
