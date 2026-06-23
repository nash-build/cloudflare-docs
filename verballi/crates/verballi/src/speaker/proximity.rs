//! Single-mic proximity ("closest to the mic") estimator.
//!
//! One microphone can't measure distance geometrically, but near-field speech
//! has tell-tale signatures we *can* read from the spectrum:
//!   * **spectral tilt / proximity effect** — close sources have relatively more
//!     low-frequency energy; distant/room sources sound thinner, and
//!   * **envelope definition** — close speech has sharp onsets, while a
//!     reverberant (far) field smears the envelope (low direct-to-reverberant
//!     ratio).
//!
//! It returns a 0..1 "closeness" score. This is a *secondary* cue — the speaker
//! gate is what reliably separates two human voices — so the pipeline weights it
//! modestly via `proximity_focus`.

pub struct Proximity {
    fast_env: f32,
    slow_env: f32,
    initialised: bool,
}

impl Proximity {
    pub fn new() -> Self {
        Self { fast_env: 0.0, slow_env: 0.0, initialised: false }
    }

    /// Estimate closeness for one magnitude frame.
    pub fn closeness(&mut self, mag: &[f32]) -> f32 {
        let n = mag.len();
        if n < 8 {
            return 1.0;
        }
        // Split low vs high band energy (split ~ 1 kHz at 16 kHz / 257 bins).
        let split = (n as f32 * 0.125) as usize; // ~1 kHz
        let mut lo = 0.0f32;
        let mut hi = 0.0f32;
        for k in 1..split {
            lo += mag[k] * mag[k];
        }
        for k in split..n {
            hi += mag[k] * mag[k];
        }
        let total = lo + hi + 1e-9;
        // Proximity effect: more low-frequency share → closer. Normalised to 0..1.
        let lf_share = (lo / total).clamp(0.0, 1.0);

        // Envelope definition via fast/slow follower ratio (direct-to-reverb proxy).
        let frame_energy = total.sqrt();
        if !self.initialised {
            self.fast_env = frame_energy;
            self.slow_env = frame_energy;
            self.initialised = true;
        }
        self.fast_env += 0.4 * (frame_energy - self.fast_env);
        self.slow_env += 0.02 * (frame_energy - self.slow_env);
        // A peaky (well-defined) envelope → fast > slow → high definition.
        let definition = (self.fast_env / (self.slow_env + 1e-9)).clamp(0.0, 2.0) / 2.0;

        // Combine: both cues matter, but keep it gentle/bounded.
        let score = 0.6 * lf_share + 0.4 * definition;
        score.clamp(0.0, 1.0)
    }
}

impl Default for Proximity {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_field_scores_higher_than_thin_distant() {
        let n = 257;
        // "Near": strong low-frequency body.
        let mut near = vec![0.01f32; n];
        for k in 1..30 {
            near[k] = 1.0;
        }
        // "Far": thin, high-tilted, little low end.
        let mut far = vec![0.01f32; n];
        for k in 150..220 {
            far[k] = 0.5;
        }

        let mut pa = Proximity::new();
        let mut pb = Proximity::new();
        // settle envelopes
        let mut sn = 0.0;
        let mut sf = 0.0;
        for _ in 0..20 {
            sn = pa.closeness(&near);
            sf = pb.closeness(&far);
        }
        assert!(sn > sf, "near {sn} far {sf}");
    }
}
