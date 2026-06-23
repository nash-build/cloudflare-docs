//! Utterance-level speaker verification and a rolling presence scorer.
//!
//! The per-frame gate ([`super::gate`]) decides identity every 16 ms, which is
//! responsive but jittery. Utterance-level models (x-vector, **ECAPA-TDNN**)
//! decide over ~1 s and are far more robust. This module runs such a model on a
//! sliding window a few times per second — off the audio hot path — and produces
//! a smoothed **presence score** in `[0, 1]` that reinforces the fast gate via
//! [`crate::Pipeline::set_speaker_presence`].
//!
//! The built-in [`MfccVerifier`] averages the engine's MFCC features over the
//! window (so its embedding space matches the enrolled [`SpeakerProfile`]
//! centroid and it's runnable today). For maximum robustness, implement
//! [`SpeakerVerifier`] around an ECAPA-TDNN ONNX model (see `docs/NEURAL_MODELS.md`)
//! and pass it to [`PresenceScorer::new`]; nothing else changes.

use crate::config::{FFT_SIZE, OUTPUT_SAMPLE_RATE};
use crate::dsp::stft::Stft;
use crate::speaker::embedding::{cosine, Embedder, MfccEmbedder};

const N_BINS: usize = FFT_SIZE / 2 + 1;

/// Produces one L2-normalised embedding for a whole audio segment (utterance).
pub trait SpeakerVerifier: Send {
    /// Embedding dimensionality.
    fn dim(&self) -> usize;
    /// Embed a segment of 16 kHz mono audio into an L2-normalised vector.
    fn embed_utterance(&mut self, audio_16k: &[f32]) -> Vec<f32>;
}

/// Built-in verifier: mean of the engine's MFCC frame embeddings over the
/// segment. Its space matches the enrolled `SpeakerProfile` centroid, so a
/// profile can be used directly as the reference.
pub struct MfccVerifier {
    embedder: MfccEmbedder,
}

impl MfccVerifier {
    pub fn new() -> Self {
        Self { embedder: MfccEmbedder::new(N_BINS, OUTPUT_SAMPLE_RATE) }
    }
}

impl Default for MfccVerifier {
    fn default() -> Self {
        Self::new()
    }
}

impl SpeakerVerifier for MfccVerifier {
    fn dim(&self) -> usize {
        self.embedder.dim()
    }

    fn embed_utterance(&mut self, audio_16k: &[f32]) -> Vec<f32> {
        // Fresh STFT per call → no state bleed between (overlapping) windows.
        let mut stft = Stft::new();
        let dim = self.embedder.dim();
        let mut sum = vec![0.0f32; dim];
        let mut count = 0u32;
        let mut sink = Vec::new();
        let embedder = &self.embedder;
        stft.process(audio_16k, &mut sink, |mag| {
            let e = embedder.embed(mag);
            for (s, v) in sum.iter_mut().zip(e.iter()) {
                *s += v;
            }
            count += 1;
            vec![1.0; mag.len()] // unity gain; we only want the analysis frames
        });
        if count > 0 {
            for s in &mut sum {
                *s /= count as f32;
            }
        }
        normalise(&mut sum);
        sum
    }
}

/// Maintains a rolling audio window and periodically re-scores speaker presence.
pub struct PresenceScorer {
    verifier: Box<dyn SpeakerVerifier>,
    reference: Vec<f32>,
    have_ref: bool,
    buf: Vec<f32>,
    window: usize, // samples kept for each re-score
    period: usize, // samples between re-scores
    since: usize,
    presence: f32,
}

impl PresenceScorer {
    /// `window_secs` ~= 1.0 (decision context), `period_secs` ~= 0.25 (cadence).
    pub fn new(verifier: Box<dyn SpeakerVerifier>, window_secs: f32, period_secs: f32) -> Self {
        let rate = OUTPUT_SAMPLE_RATE as f32;
        Self {
            verifier,
            reference: Vec::new(),
            have_ref: false,
            buf: Vec::new(),
            window: (window_secs * rate) as usize,
            period: (period_secs * rate).max(1.0) as usize,
            since: 0,
            presence: 1.0,
        }
    }

    /// Set the reference from an enrollment clip.
    pub fn set_reference_audio(&mut self, audio_16k: &[f32]) {
        self.reference = self.verifier.embed_utterance(audio_16k);
        self.have_ref = true;
    }

    /// Set the reference directly from an embedding (e.g. a `SpeakerProfile`
    /// centroid, which is in the `MfccVerifier` space).
    pub fn set_reference_embedding(&mut self, mut emb: Vec<f32>) {
        normalise(&mut emb);
        self.reference = emb;
        self.have_ref = true;
    }

    /// Current smoothed presence score in `[0, 1]`.
    pub fn presence(&self) -> f32 {
        self.presence
    }

    /// Push freshly produced 16 kHz mono audio. Returns `Some(score)` when a
    /// re-score happened this call, else `None`.
    pub fn push(&mut self, audio_16k: &[f32]) -> Option<f32> {
        if !self.have_ref {
            return None;
        }
        self.buf.extend_from_slice(audio_16k);
        if self.buf.len() > self.window {
            let drop = self.buf.len() - self.window;
            self.buf.drain(..drop);
        }
        self.since += audio_16k.len();
        if self.since < self.period || self.buf.len() < self.period {
            return None;
        }
        self.since = 0;

        let emb = self.verifier.embed_utterance(&self.buf);
        let sim = cosine(&self.reference, &emb); // -1..1
        // Soft map: ~0.7 cosine is the "it's you" knee.
        let score = sigmoid((sim - 0.7) * 8.0);
        // Smooth across re-scores.
        self.presence += 0.5 * (score - self.presence);
        Some(self.presence)
    }
}

#[inline]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn normalise(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 1e-9 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voiced(f0: f32, n: usize, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = i as f32 / OUTPUT_SAMPLE_RATE as f32;
                let s: f32 = (1..7).map(|k| (1.0 / k as f32) * (2.0 * std::f32::consts::PI * f0 * k as f32 * t).sin()).sum();
                amp * s / 2.5
            })
            .collect()
    }

    #[test]
    fn presence_high_for_enrolled_low_for_other() {
        let mut scorer = PresenceScorer::new(Box::new(MfccVerifier::new()), 1.0, 0.25);
        scorer.set_reference_audio(&voiced(150.0, 16_000, 0.4)); // 1 s of "me"

        // Feed "me" until a re-score fires.
        let mut me_score = None;
        for _ in 0..10 {
            if let Some(s) = scorer.push(&voiced(150.0, 4_000, 0.4)) {
                me_score = Some(s);
            }
        }
        let me = me_score.expect("expected a re-score for enrolled voice");

        // Now feed a clearly different voice.
        let mut other_score = me;
        for _ in 0..20 {
            if let Some(s) = scorer.push(&voiced(280.0, 4_000, 0.4)) {
                other_score = s;
            }
        }
        assert!(me > 0.6, "enrolled presence too low: {me}");
        assert!(other_score < me - 0.15, "other voice not separated: me {me} other {other_score}");
    }

    #[test]
    fn no_score_without_reference() {
        let mut scorer = PresenceScorer::new(Box::new(MfccVerifier::new()), 1.0, 0.25);
        assert!(scorer.push(&vec![0.1; 8_000]).is_none());
    }
}
