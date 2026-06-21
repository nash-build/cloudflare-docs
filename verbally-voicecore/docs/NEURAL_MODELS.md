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

## 1. DeepFilterNet → the `Enhancer` seam

DeepFilterNet is itself written in Rust and runs its own internal STFT + ERB
deep filtering with look-ahead, so it belongs at the **time-domain** seam (before
our STFT), not the per-bin mask seam.

```rust
use voicecore::{Config, VoiceEngine};
use voicecore::dsp::enhance::ClosureEnhancer;

// Pseudocode around the `df` crate (DeepFilterNet). Load the model once.
let mut df = df::tract::DfTract::new(/* model dir, params */)?;

let mut engine = VoiceEngine::new({
    let mut c = Config::default();
    c.input_sample_rate = 48_000;
    c.denoise_strength = 0.25;     // let the model do most of the work
    c
})?;

engine.pipeline_mut().set_enhancer(Box::new(ClosureEnhancer::new(
    move |frame_16k: &[f32]| df.process(frame_16k).unwrap_or_else(|_| frame_16k.to_vec())
)));

// From here, engine.process(mic) returns DeepFilterNet-cleaned + speaker-gated audio.
```

Notes:
- DeepFilterNet expects 48 kHz internally in some builds; if so, feed it before
  our 16 kHz resample by hosting it in the shell, or use a 16 kHz-capable build.
  The `Enhancer` runs *after* our resample (16 kHz), which matches DFN's
  streaming 16 kHz API.
- It buffers (look-ahead); returning a different sample count than the input is
  fine — our STFT is fully streaming.

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

### b) Runtime scoring (rolling window)
Because ECAPA is utterance-level, score it on a sliding ~0.75–1.0 s buffer a few
times per second (off the audio hot path) and let it modulate the gate, while the
fast MFCC gate handles per-frame decisions:

```text
ring buffer (last ~1 s) ──every ~250 ms──▶ ECAPA ─▶ cosine vs enrolled d-vector
                                                     │
                                       presence score (0..1) ──▶ scales speaker_focus
```

This gives ECAPA-grade identity robustness without running a heavy model every
16 ms. (A small `set_speaker_presence(score)` hook on the pipeline is the natural
place to feed it — straightforward to add when you wire the model.)

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
