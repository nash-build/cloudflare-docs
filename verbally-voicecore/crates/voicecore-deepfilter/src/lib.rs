//! DeepFilterNet enhancer for `voicecore`.
//!
//! This wraps the open [`deep_filter`](https://crates.io/crates/deep_filter)
//! crate (from the official DeepFilterNet repo, MIT/Apache) and exposes it as a
//! [`voicecore`] [`Enhancer`] — the time-domain seam in the pipeline.
//!
//! ## Two tiers, one seam
//!
//! `deep_filter` ships DeepFilterNet's **DSP front-end**: the exact ERB
//! filterbank and streaming STFT analysis/synthesis the network is built around.
//! It does **not** bundle the trained model weights. So this crate gives you:
//!
//! 1. [`ErbEnhancer`] — a **runnable today** enhancer that computes per-ERB-band
//!    suppression gains with DSP (a Wiener filter over a tracked noise floor) on
//!    DeepFilterNet's own band structure. No weights required.
//! 2. The explicit slot where the **trained** DeepFilterNet model plugs in: the
//!    network predicts the per-band gains (and deep-filter coefficients) that
//!    [`ErbEnhancer`] currently derives by DSP. Implement [`BandGainModel`]
//!    around the model run through `tract` and pass it in — the analysis,
//!    band layout, and synthesis are already wired and identical to upstream.
//!
//! Tier 1 is a real improvement and is fully tested here; tier 2 is the path to
//! full DeepFilterNet quality once you supply the model export on your build
//! host. Either way: clean-room, open source, no proprietary code.

use df::{Complex32, DFState};
use voicecore::dsp::enhance::Enhancer;

/// Predicts per-ERB-band gains for one frame. The trained DeepFilterNet model
/// implements this; [`DspBandGains`] is the built-in DSP fallback.
pub trait BandGainModel: Send {
    /// Given the per-band energy of the current frame, return a per-band gain in
    /// `[0, 1]` of the same length.
    fn gains(&mut self, band_energy: &[f32]) -> Vec<f32>;
}

/// Built-in DSP band-gain estimator: a decision-directed Wiener filter over a
/// recursively tracked per-band noise floor. Effective on stationary and
/// quasi-stationary noise; the trained model is the upgrade for the rest.
pub struct DspBandGains {
    noise: Vec<f32>,
    prev_gain: Vec<f32>,
    prev_snr: Vec<f32>,
    over_subtraction: f32,
    floor: f32,
}

impl DspBandGains {
    /// `strength` 0..1 sets aggressiveness (the residual floor at full strength).
    pub fn new(strength: f32) -> Self {
        let s = strength.clamp(0.0, 1.0);
        Self {
            noise: Vec::new(),
            prev_gain: Vec::new(),
            prev_snr: Vec::new(),
            over_subtraction: 1.0 + 1.5 * s,
            floor: (1.0 - s).max(0.03),
        }
    }
}

impl BandGainModel for DspBandGains {
    fn gains(&mut self, band_energy: &[f32]) -> Vec<f32> {
        let n = band_energy.len();
        if self.noise.len() != n {
            self.noise = band_energy.to_vec();
            self.prev_gain = vec![1.0; n];
            self.prev_snr = vec![1.0; n];
        }
        let mut out = vec![0.0f32; n];
        for b in 0..n {
            let e = band_energy[b].max(1e-12);
            // Track the noise floor: fall fast toward quieter frames, rise slowly.
            if e < self.noise[b] {
                self.noise[b] = 0.9 * self.noise[b] + 0.1 * e;
            } else {
                self.noise[b] *= 1.0008;
            }
            let post = e / (self.over_subtraction * self.noise[b] + 1e-12);
            // Decision-directed a-priori SNR smoothing (reduces musical noise).
            let prior = 0.96 * self.prev_snr[b] + 0.04 * (post - 1.0).max(0.0);
            self.prev_snr[b] = prior;
            let g = (prior / (1.0 + prior)).clamp(self.floor, 1.0);
            // Temporal smoothing of the gain.
            self.prev_gain[b] = 0.6 * self.prev_gain[b] + 0.4 * g;
            out[b] = self.prev_gain[b];
        }
        out
    }
}

/// Streaming enhancer over DeepFilterNet's ERB band structure.
pub struct ErbEnhancer {
    state: DFState,
    erb: Vec<usize>, // freq-bin count per ERB band
    freq_size: usize,
    hop: usize,
    model: Box<dyn BandGainModel>,
    in_buf: Vec<f32>,
    spec: Vec<Complex32>,
    band_energy: Vec<f32>,
}

impl ErbEnhancer {
    /// 16 kHz enhancer (fft 512 / hop 256 / 32 ERB bands) using the built-in DSP
    /// band gains at the given `strength` (0..1). Matches the engine's 16 kHz
    /// working rate, so it sits naturally in the `Enhancer` seam.
    pub fn new_dsp(strength: f32) -> Self {
        Self::with_model(Box::new(DspBandGains::new(strength)))
    }

    /// Use a custom band-gain model — e.g. the trained DeepFilterNet network run
    /// through `tract`. Analysis/synthesis and the ERB layout are upstream's.
    pub fn with_model(model: Box<dyn BandGainModel>) -> Self {
        // 16 kHz, fft 512, hop 256, 32 bands, min 2 freqs/band (hop*2 <= fft).
        let state = DFState::new(16_000, 512, 256, 32, 2);
        let erb = state.erb.clone();
        let freq_size = state.freq_size;
        let hop = state.frame_size;
        let nb_bands = erb.len();
        Self {
            state,
            erb,
            freq_size,
            hop,
            model,
            in_buf: Vec::with_capacity(1024),
            spec: vec![Complex32::new(0.0, 0.0); freq_size],
            band_energy: vec![0.0; nb_bands],
        }
    }

    fn process_one_frame(&mut self, frame: &[f32], out: &mut Vec<f32>) {
        // STFT analysis (handles overlap memory internally).
        self.state.analysis(frame, &mut self.spec);

        // Per-band energy.
        let mut k = 0usize;
        for (b, &width) in self.erb.iter().enumerate() {
            let mut e = 0.0f32;
            for _ in 0..width {
                if k < self.freq_size {
                    let c = self.spec[k];
                    e += c.re * c.re + c.im * c.im;
                }
                k += 1;
            }
            self.band_energy[b] = e;
        }

        // Band gains from the model (DSP or trained), then expand to bins.
        let gains = self.model.gains(&self.band_energy);
        let mut k = 0usize;
        for (b, &width) in self.erb.iter().enumerate() {
            let g = gains.get(b).copied().unwrap_or(1.0);
            for _ in 0..width {
                if k < self.freq_size {
                    self.spec[k].re *= g;
                    self.spec[k].im *= g;
                }
                k += 1;
            }
        }

        // ISTFT synthesis → hop samples of clean audio.
        let mut frame_out = vec![0.0f32; self.hop];
        self.state.synthesis(&mut self.spec, &mut frame_out);
        out.extend_from_slice(&frame_out);
    }
}

impl Enhancer for ErbEnhancer {
    fn process(&mut self, input: &[f32]) -> Vec<f32> {
        self.in_buf.extend_from_slice(input);
        let mut out = Vec::with_capacity(self.in_buf.len());
        while self.in_buf.len() >= self.hop {
            let frame: Vec<f32> = self.in_buf[..self.hop].to_vec();
            self.process_one_frame(&frame, &mut out);
            self.in_buf.drain(..self.hop);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use voicecore::{Config, VoiceEngine};

    fn white_noise(n: usize) -> Vec<f32> {
        let mut s = 99u32;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                ((s >> 9) as f32 / (1 << 23) as f32 - 1.0) * 0.2
            })
            .collect()
    }

    #[test]
    fn reduces_noise_energy() {
        let mut e = ErbEnhancer::new_dsp(0.9);
        let noise = white_noise(32_000);
        let out = e.process(&noise);
        let ein: f32 = noise.iter().map(|x| x * x).sum::<f32>() / noise.len() as f32;
        let eout: f32 = out.iter().map(|x| x * x).sum::<f32>() / out.len().max(1) as f32;
        assert!(eout < ein, "noise energy not reduced: in {ein} out {eout}");
    }

    #[test]
    fn streaming_handles_arbitrary_chunks() {
        let mut e = ErbEnhancer::new_dsp(0.7);
        let sig = white_noise(20_000);
        let mut total = 0usize;
        for chunk in sig.chunks(101) {
            total += e.process(chunk).len();
        }
        assert!(total > 0 && total <= sig.len());
    }

    #[test]
    fn integrates_as_engine_enhancer() {
        let mut cfg = Config::default();
        cfg.input_sample_rate = 16_000;
        cfg.denoise_strength = 0.2; // let the enhancer carry the load
        let mut engine = VoiceEngine::new(cfg).unwrap();
        engine
            .pipeline_mut()
            .set_enhancer(Box::new(ErbEnhancer::new_dsp(0.85)));
        let out = engine.process(&white_noise(16_000));
        assert!(!out.is_empty());
        assert!(out.iter().all(|x| x.is_finite()));
    }
}
