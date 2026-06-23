# How real-time voice isolation works (and how we replicate the *results* cleanly)

This is a from-public-knowledge brief on the techniques behind modern single-mic
voice isolation (the category Krisp, NVIDIA Broadcast, Google Meet "denoise",
and Discord's Krisp integration sit in), and how each maps onto `verbally`. It
cites open research and open implementations only — no proprietary SDK was
inspected to write it.

## The problem, precisely

From **one** microphone, in real time (≤ ~30 ms added latency), output **only the
primary near-field talker** and remove:
1. **Stationary noise** — fans, HVAC, hiss, hum, car interior.
2. **Non-stationary noise** — keyboard, dishes, dog, traffic transients.
3. **Other people's voices** — the hard one; generic denoisers *keep* it because
   it looks like speech.

(1) and (2) are *noise suppression*. (3) is *target-speaker extraction*. The
products that feel "magic" do both. So do we.

## Technique lineage (all public)

### Noise suppression
| Approach | Idea | Notes |
|----------|------|-------|
| Spectral subtraction / Wiener | estimate noise spectrum, attenuate per bin | classic DSP; great for stationary noise, "musical noise" artifacts if pushed |
| **RNNoise** (Valin, 2017) | tiny RNN predicts per-band gains over DSP features | the open gateway model; ~ realtime on a CPU thread |
| **DTLN** (Westhausen, 2020) | dual-signal stacked LSTM, magnitude + learned | strong, small, streaming |
| **DeepFilterNet 2/3** (Schröter, 2022–23) | ERB-band gains **+ deep filtering** of low bands | near-SOTA quality, real-time on CPU, **Rust**, MIT/Apache |
| **PercepNet / DeepFilter family** | perceptually-weighted band gains | the design philosophy behind several commercial denoisers |

**Our mapping:** the built-in `dsp::denoise` is a decision-directed Wiener
suppressor (Ephraim–Malah style) over a VAD-gated noise estimate — a real,
shippable baseline for (1)/(2). For SOTA, drop **DeepFilterNet** into the
`dsp::enhance::Enhancer` seam (time-domain in/out, its natural shape). See
[`NEURAL_MODELS.md`](NEURAL_MODELS.md).

### Target-speaker extraction (the "keep only me" part)
| Approach | Idea |
|----------|------|
| Speaker embeddings — **d-vector**, **x-vector**, **ECAPA-TDNN** | map a voice to a point in an identity space; same speaker → close, different → far |
| **VoiceFilter** (Wang, 2019) | condition a separation mask on the target's d-vector |
| **VoiceFilter-Lite** (2020) | streaming, on-device version that *suppresses* non-target speech rather than fully separating |
| **Personal VAD** (2019) | VAD that only fires for the enrolled speaker |

The pattern is always: **(a) enroll** the target once → an embedding; **(b) at
runtime** compare incoming audio to that embedding and keep what matches.

**Our mapping:**
- **Embedding:** `speaker::embedding::MfccEmbedder` (default) — mel filterbank →
  log → DCT, the same feature family ECAPA front-ends use, cepstral-mean-
  subtracted + L2-normalised. Swap in **ECAPA-TDNN (ONNX)** for maximum
  separation between similar voices.
- **Enrollment:** `SpeakerProfile` averages embeddings into a centroid (your
  fingerprint), persisted to bytes.
- **Runtime gate:** `speaker::gate` compares each frame to the centroid (cosine),
  smooths it, and maps it through a focus-shaped soft threshold to a gain — i.e. a
  streaming, suppression-style target-speaker gate in the VoiceFilter-Lite
  spirit.

### Near-field / "closest to the mic"
Distance can't be measured by one mic geometrically, so use acoustic cues:
- **Direct-to-reverberant ratio (DRR)** — close = dry, far = roomy/smeared.
- **Proximity effect** — close sources have boosted low frequencies.
- **Envelope definition** — close speech has sharp onsets.

**Our mapping:** `speaker::proximity` estimates closeness from spectral tilt +
fast/slow envelope ratio, folded into the gate as a secondary cue. It's a helper,
not the decider — the embedding lock is what separates two humans.

## The full chain in `verbally`

```
mic → resample(16k) → high-pass → [Enhancer: DeepFilterNet] → STFT
        → VAD ─┬─ Wiener mask ─┐
               └─ speaker gate (MFCC/ECAPA identity × proximity) ┘ × per-bin
        → ISTFT → AGC/limiter → 16k mono → ElevenLabs
```

## What makes the commercial ones feel better — and how we close the gap

1. **A trained denoiser** instead of DSP → biggest perceptual jump. *Action:*
   DeepFilterNet via the `Enhancer` seam.
2. **A trained speaker model** (ECAPA) instead of MFCC → cleanly separates
   similar voices. *Action:* ONNX ECAPA via the `Embedder` seam / verifier.
3. **A trained mask conditioned on the embedding** (VoiceFilter-Lite) →
   separates *simultaneous* overlapping voices, not just alternating ones. This
   is the one piece that is genuinely a learned model; our heuristic gate handles
   the common alternating case, and this is the upgrade path for full overlap.
4. **Tuning for the downstream consumer** — for us that's ElevenLabs' VAD/turn
   detection; we keep a gate floor and moderate suppression so turns stay snappy
   (see [`ELEVENLABS.md`](ELEVENLABS.md)).

## Latency budget

| Stage | Added latency |
|-------|---------------|
| STFT (512 @ 16k, 50% hop) | ~32 ms |
| DeepFilterNet (if used) | ~20–40 ms (model look-ahead) |
| ECAPA scoring | runs on a rolling window, off the hot path |

Conversational targets are ~100–200 ms end-to-end, so a DSP-only build is very
comfortable and a single neural stage still fits.

## Bottom line

The recipe is public: **trained denoiser + speaker-embedding lock + light
near-field cues + careful real-time engineering.** `verbally` implements that
shape today with open DSP and gives you typed seams to drop the open trained
models (DeepFilterNet, ECAPA) into the exact right places — reproducing the
*results* without anyone's proprietary code.
