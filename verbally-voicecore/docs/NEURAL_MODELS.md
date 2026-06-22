# Dropping in the open neural models

The engine ships with a working DSP path and **typed seams** so you can add the
open SOTA models in the right places. All models below are permissively licensed
and you obtain the weights yourself — nothing proprietary is bundled.

| Capability | Seam (trait) | Open model | License |
|------------|--------------|------------|---------|
| Neural denoise | `dsp::enhance::Enhancer` (time-domain) | **DeepFilterNet 3** | MIT/Apache (Rust) |
| Strong speaker embedding | `speaker::embedding::Embedder` / verifier | **ECAPA-TDNN** (ONNX) | varies (e.g. SpeechBrain MIT) |
| Overlap separation | (future) embedding-conditioned mask | **VoiceFilter-Lite** style | train your own |

> Cite and comply with each model's license. Verify the license of any specific
> checkpoint before shipping commercially.

## 1. DeepFilterNet → the `voicecore-deepfilter` crate

DeepFilterNet runs its own STFT + **ERB-band** deep filtering, so it belongs at
the **time-domain** `Enhancer` seam (before our STFT), not the per-bin mask seam.
This is shipped as the `voicecore-deepfilter` crate, built on the official open
`deep_filter` crate (MIT/Apache, package `deep_filter`, lib `df`).

It comes in two tiers sharing one seam:

**Tier 1 — runnable today (no weights).** `ErbEnhancer::new_dsp(strength)` uses
DeepFilterNet's exact ERB filterbank + streaming analysis/synthesis and computes
per-band suppression gains by DSP (decision-directed Wiener over a tracked noise
floor). It's tested and usable right now:

```rust
use voicecore::{Config, VoiceEngine};
use voicecore_deepfilter::ErbEnhancer;

let mut engine = VoiceEngine::new({
    let mut c = Config::default();
    c.input_sample_rate = 48_000;
    c.denoise_strength = 0.2;      // let the enhancer carry the load
    c
})?;
engine.pipeline_mut().set_enhancer(Box::new(ErbEnhancer::new_dsp(0.9)));
// engine.process(mic) → ERB-enhanced + speaker-gated 16 kHz mono.
```

Or from the CLI: `voicecore process in.wav out.wav --deepfilter 0.9 --denoise 0.2`.

**Tier 2 — full neural quality (bring the model).** The trained network predicts
the per-band gains (and deep-filter coefficients) that Tier 1 derives by DSP.
Implement the `BandGainModel` trait around the model run through `tract`, and the
identical ERB layout + analysis/synthesis are reused unchanged:

```rust
use voicecore_deepfilter::{BandGainModel, ErbEnhancer};

struct TractDfn { /* tract model, state */ }
impl BandGainModel for TractDfn {
    fn gains(&mut self, band_energy: &[f32]) -> Vec<f32> {
        // run the DeepFilterNet ERB-gain stage; return per-band gains in [0,1]
        self.model.run(band_energy)
    }
}

engine.pipeline_mut().set_enhancer(Box::new(ErbEnhancer::with_model(Box::new(TractDfn::new()?))));
```

Notes:
- The enhancer runs at 16 kHz (matching our working rate). Obtain DeepFilterNet
  model exports from the upstream repo; comply with their license.
- It buffers in hop-sized frames; returning a different sample count than the
  input is fine — our STFT is fully streaming.

## 2. ECAPA-TDNN → stronger "lock onto my voice"

ECAPA produces an **utterance-level** embedding (e.g. 192-d), not a per-16ms-frame
descriptor. So it integrates at two cadences:

### a) Enrollment (compute the profile with ECAPA)
Run ECAPA over the whole enrollment clip, install the resulting vector as the
profile centroid:

```rust
use voicecore::SpeakerProfile;

let dvec: Vec<f32> = ecapa.embed_utterance(&enrollment_16k)?; // 192-d, via ort
let profile = SpeakerProfile::from_centroid(dvec);            // L2-normalised
engine.set_profile(&profile);
```

### b) Runtime scoring (rolling window) — implemented

This loop is already built: `speaker::verify::PresenceScorer` keeps a rolling
window, re-scores every ~250 ms off the hot path, and feeds the result to
`Pipeline::set_speaker_presence`, while the fast MFCC gate handles per-frame
decisions. The `live` app wires it automatically when a `--profile` is given.

```text
ring buffer (last ~1 s) ──every ~250 ms──▶ SpeakerVerifier ─▶ cosine vs reference
                                                              │
                                          presence score (0..1) ─▶ set_speaker_presence
```

```rust
use voicecore::speaker::verify::{PresenceScorer, SpeakerVerifier};

// Swap the built-in MfccVerifier for ECAPA by implementing SpeakerVerifier:
struct EcapaVerifier { session: ort::session::Session }
impl SpeakerVerifier for EcapaVerifier {
    fn dim(&self) -> usize { 192 }
    fn embed_utterance(&mut self, audio_16k: &[f32]) -> Vec<f32> { embed_utterance(&self.session, audio_16k).unwrap() }
}

let mut scorer = PresenceScorer::new(Box::new(EcapaVerifier { session }), 1.0, 0.25);
scorer.set_reference_audio(&enrollment_16k);   // ECAPA d-vector of the enrollee
// per chunk: if let Some(score) = scorer.push(&clean) { engine.set_speaker_presence(score); }
```

The built-in `MfccVerifier` works today (its space matches the `SpeakerProfile`
centroid, so a saved profile is a ready reference); ECAPA is a drop-in upgrade of
just the verifier.

### Running ECAPA ONNX with `ort`

```toml
# Cargo.toml (in your shell crate, behind a `neural` feature)
ort = { version = "2", features = ["ndarray"] }   # ONNX Runtime
ndarray = "0.16"
```

```rust
use ort::{session::Session, value::Tensor};

let session = Session::builder()?
    .with_execution_providers([
        // CoreML on Apple, NNAPI on Android, CPU fallback elsewhere.
        ort::execution_providers::CoreMLExecutionProvider::default().build(),
    ])?
    .commit_from_file("ecapa_tdnn.onnx")?;

fn embed_utterance(session: &Session, audio_16k: &[f32]) -> anyhow::Result<Vec<f32>> {
    // Most ECAPA exports take log-mel features [1, T, 80]; some take raw audio.
    let feats = log_mel_80(audio_16k);                 // shape [T, 80]
    let input = Tensor::from_array(([1, feats.nrows(), 80], feats.into_raw_vec()))?;
    let out = session.run(ort::inputs![input]?)?;
    let emb = out[0].try_extract_tensor::<f32>()?.1.to_vec();
    Ok(l2_normalise(emb))
}
```

Pick a checkpoint whose expected input you control (raw-audio ECAPA exports are
simplest; otherwise reuse the engine's mel front-end). Validate the embedding
dimension matches what you install as the profile.

## 3. Overlapping voices (the genuine ML piece)

Separating two people talking **at the same time** needs an embedding-conditioned
mask — **VoiceFilter-Lite**. There's no open drop-in checkpoint that's plug-and-
play across domains, so this is a *train-it* item:

1. Take a denoiser/mask backbone (DTLN/DeepFilterNet-style).
2. Concatenate the target d-vector to each frame's features.
3. Train on mixtures `(target + interferer + noise) → target` with your data.
4. Host it at the `dsp::denoise::NeuralDenoiser` seam (per-bin mask on our STFT)
   or as an `Enhancer` if it's time-domain.

Until then, the heuristic gate cleanly handles the **alternating-speaker** case
(someone else talks while you're quiet), which covers most real call scenarios.

## Performance budget

The DSP path runs ~0.001× real time (`voicecore bench`). Typical added cost:
DeepFilterNet ~0.1–0.3× RTF on a modern CPU core; ECAPA on a 1 s window a few
times/second is negligible amortised. Use the per-platform execution providers
(CoreML/NNAPI) on mobile to keep battery and latency in budget.
