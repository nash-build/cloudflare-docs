//! Target-speaker gate: keep the enrolled voice, attenuate everyone else.
//!
//! Per frame we get a cosine similarity to the enrolled centroid and a proximity
//! score. We smooth the similarity (single frames are noisy), map it through a
//! soft threshold shaped by `speaker_focus`, fold in proximity, and clamp to a
//! floor so a momentary mismatch never produces dead silence (which would break
//! ElevenLabs' turn detection).

use super::embedding::{cosine, Embedder, SpeakerProfile};
use super::proximity::Proximity;

pub struct SpeakerGate {
    centroid: Vec<f32>,
    enrolled: bool,
    smoothed_sim: f32,
    proximity: Proximity,
    smoothed_gain: f32,
}

impl SpeakerGate {
    pub fn new(dim: usize) -> Self {
        Self {
            centroid: vec![0.0; dim],
            enrolled: false,
            smoothed_sim: 0.0,
            proximity: Proximity::new(),
            smoothed_gain: 1.0,
        }
    }

    /// Install an enrolled profile.
    pub fn set_profile(&mut self, profile: &SpeakerProfile) {
        self.centroid = profile.centroid();
        self.enrolled = true;
        self.smoothed_sim = 0.5;
    }

    pub fn is_enrolled(&self) -> bool {
        self.enrolled
    }

    /// Compute the broadband gain to apply to this frame.
    ///
    /// * `embedder`/`mag` — to embed the current frame.
    /// * `speaker_focus`  — 0 (let all through) .. 1 (hard reject non-you).
    /// * `proximity_focus`— 0 (ignore distance) .. 1 (favour close sources).
    /// * `floor`          — minimum gain ever returned.
    pub fn gain(
        &mut self,
        embedder: &dyn Embedder,
        mag: &[f32],
        speaker_focus: f32,
        proximity_focus: f32,
        floor: f32,
    ) -> f32 {
        // Proximity is always tracked so its envelope followers stay warm.
        let close = self.proximity.closeness(mag);

        if !self.enrolled || speaker_focus <= 0.0 {
            // No profile yet → don't gate on identity; optionally still favour
            // close sources if asked.
            let g = if proximity_focus > 0.0 {
                blend(1.0, close, proximity_focus).max(floor)
            } else {
                1.0
            };
            self.smoothed_gain += 0.3 * (g - self.smoothed_gain);
            return self.smoothed_gain;
        }

        let emb = embedder.embed(mag);
        let sim = cosine(&self.centroid, &emb); // -1..1
        // Map to 0..1 and smooth across frames.
        let sim01 = (sim * 0.5 + 0.5).clamp(0.0, 1.0);
        self.smoothed_sim += 0.25 * (sim01 - self.smoothed_sim);

        // Soft threshold: as focus rises, the acceptance curve steepens and the
        // midpoint moves up, so non-matching voices fall off faster.
        let midpoint = 0.5 + 0.2 * speaker_focus;
        let steepness = 4.0 + 16.0 * speaker_focus;
        let identity_gain = sigmoid((self.smoothed_sim - midpoint) * steepness);

        // Fold in proximity as a gentle multiplier (weighted by its focus).
        let prox_gain = blend(1.0, close, proximity_focus);
        let mut g = identity_gain * prox_gain;

        // Blend back toward unity by (1 - focus) so low focus is gentle, and
        // clamp to the floor.
        g = blend(1.0, g, speaker_focus).clamp(floor, 1.0);

        self.smoothed_gain += 0.3 * (g - self.smoothed_gain);
        self.smoothed_gain
    }
}

#[inline]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// Linear blend: `amount`=0 → `a`, `amount`=1 → `b`.
#[inline]
fn blend(a: f32, b: f32, amount: f32) -> f32 {
    a + (b - a) * amount.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::super::embedding::BandEmbedder;
    use super::*;

    fn voice_low(n: usize) -> Vec<f32> {
        let mut m = vec![0.02f32; n];
        for k in 5..25 {
            m[k] = 1.0;
        }
        m
    }
    fn voice_high(n: usize) -> Vec<f32> {
        let mut m = vec![0.02f32; n];
        for k in 120..170 {
            m[k] = 1.0;
        }
        m
    }

    #[test]
    fn passes_enrolled_voice_rejects_other() {
        let n = 257;
        let emb = BandEmbedder::new(n);
        let mut profile = SpeakerProfile::new(emb.dim());
        for _ in 0..30 {
            profile.add(&emb.embed(&voice_low(n)));
        }
        let mut gate = SpeakerGate::new(emb.dim());
        gate.set_profile(&profile);

        // Settle on the enrolled voice.
        let mut g_self = 0.0;
        for _ in 0..20 {
            g_self = gate.gain(&emb, &voice_low(n), 0.9, 0.0, 0.05);
        }
        // Switch to a different voice.
        let mut g_other = 0.0;
        for _ in 0..20 {
            g_other = gate.gain(&emb, &voice_high(n), 0.9, 0.0, 0.05);
        }
        assert!(g_self > 0.7, "enrolled voice attenuated: {g_self}");
        assert!(g_other < 0.3, "other voice not rejected: {g_other}");
    }

    #[test]
    fn never_below_floor() {
        let n = 257;
        let emb = BandEmbedder::new(n);
        let mut profile = SpeakerProfile::new(emb.dim());
        for _ in 0..30 {
            profile.add(&emb.embed(&voice_low(n)));
        }
        let mut gate = SpeakerGate::new(emb.dim());
        gate.set_profile(&profile);
        let g = gate.gain(&emb, &voice_high(n), 1.0, 0.0, 0.1);
        assert!(g >= 0.1 - 1e-6);
    }

    #[test]
    fn unenrolled_passes_through() {
        let n = 257;
        let emb = BandEmbedder::new(n);
        let mut gate = SpeakerGate::new(emb.dim());
        let g = gate.gain(&emb, &voice_low(n), 0.9, 0.0, 0.05);
        assert!(g > 0.9, "unenrolled should pass: {g}");
    }
}
