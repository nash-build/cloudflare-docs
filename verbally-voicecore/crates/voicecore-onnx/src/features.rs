//! Log-mel feature front-end for speaker models (ECAPA-TDNN, x-vector).
//!
//! Most ECAPA ONNX exports take log-mel filterbank features, typically 80 mels
//! over 25 ms windows hopped by 10 ms at 16 kHz. This module produces exactly
//! that, in pure Rust, so it builds everywhere (no ONNX Runtime needed) and can
//! be unit-tested. The `ort`-gated verifier feeds these features to the model.

use rustfft::{num_complex::Complex32, FftPlanner};

/// Feature-extraction parameters. Defaults match the common 16 kHz / 80-mel
/// ECAPA recipe; adjust to whatever your specific checkpoint expects.
#[derive(Debug, Clone)]
pub struct MelConfig {
    pub sample_rate: u32,
    pub n_fft: usize,
    pub hop: usize,
    pub n_mels: usize,
    pub fmin: f32,
    pub fmax: f32,
}

impl Default for MelConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            n_fft: 400, // 25 ms
            hop: 160,   // 10 ms
            n_mels: 80,
            fmin: 20.0,
            fmax: 7600.0,
        }
    }
}

/// Computes log-mel features frame by frame.
pub struct MelFrontend {
    cfg: MelConfig,
    window: Vec<f32>,
    filters: Vec<(usize, Vec<f32>)>, // per mel: (first_bin, weights)
    fft: std::sync::Arc<dyn rustfft::Fft<f32>>,
    n_bins: usize,
}

impl MelFrontend {
    pub fn new(cfg: MelConfig) -> Self {
        let n_bins = cfg.n_fft / 2 + 1;
        // Periodic Hann window.
        let window: Vec<f32> = (0..cfg.n_fft)
            .map(|n| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * n as f32 / cfg.n_fft as f32).cos())
            .collect();

        // Mel filterbank.
        let (mmin, mmax) = (hz_to_mel(cfg.fmin), hz_to_mel(cfg.fmax));
        let edges: Vec<f32> = (0..cfg.n_mels + 2)
            .map(|i| mel_to_hz(mmin + (mmax - mmin) * i as f32 / (cfg.n_mels + 1) as f32))
            .collect();
        let bin_of = |hz: f32| hz * (cfg.n_fft as f32) / cfg.sample_rate as f32;
        let mut filters = Vec::with_capacity(cfg.n_mels);
        for m in 1..=cfg.n_mels {
            let (l, c, r) = (bin_of(edges[m - 1]), bin_of(edges[m]), bin_of(edges[m + 1]));
            let start = l.floor().max(0.0) as usize;
            let end = (r.ceil() as usize).min(n_bins - 1);
            let mut w = Vec::with_capacity(end.saturating_sub(start) + 1);
            for k in start..=end {
                let kf = k as f32;
                let v = if kf <= c {
                    if c > l { (kf - l) / (c - l) } else { 0.0 }
                } else if r > c {
                    (r - kf) / (r - c)
                } else {
                    0.0
                };
                w.push(v.clamp(0.0, 1.0));
            }
            filters.push((start, w));
        }

        let fft = FftPlanner::<f32>::new().plan_fft_forward(cfg.n_fft);
        Self { cfg, window, filters, fft, n_bins }
    }

    /// Number of mel channels (feature dimension per frame).
    pub fn n_mels(&self) -> usize {
        self.cfg.n_mels
    }

    /// Number of frames produced for `len` input samples.
    pub fn n_frames(&self, len: usize) -> usize {
        if len < self.cfg.n_fft {
            0
        } else {
            (len - self.cfg.n_fft) / self.cfg.hop + 1
        }
    }

    /// Compute log-mel features, returned row-major as `[n_frames * n_mels]`
    /// along with `n_frames`.
    pub fn features(&self, audio: &[f32]) -> (Vec<f32>, usize) {
        let frames = self.n_frames(audio.len());
        let mut out = vec![0.0f32; frames * self.cfg.n_mels];
        let mut buf = vec![Complex32::new(0.0, 0.0); self.cfg.n_fft];

        for t in 0..frames {
            let start = t * self.cfg.hop;
            for i in 0..self.cfg.n_fft {
                buf[i] = Complex32::new(audio[start + i] * self.window[i], 0.0);
            }
            self.fft.process(&mut buf);

            let row = &mut out[t * self.cfg.n_mels..(t + 1) * self.cfg.n_mels];
            for (m, (s, weights)) in self.filters.iter().enumerate() {
                let mut acc = 0.0f32;
                for (j, &w) in weights.iter().enumerate() {
                    let k = s + j;
                    if k < self.n_bins {
                        let p = buf[k].norm_sqr();
                        acc += w * p;
                    }
                }
                row[m] = (acc + 1e-10).ln();
            }
        }
        (out, frames)
    }
}

#[inline]
fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}
#[inline]
fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10f32.powf(mel / 2595.0) - 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_count_and_shape_are_correct() {
        let fe = MelFrontend::new(MelConfig::default());
        let audio = vec![0.1f32; 16_000]; // 1 s
        let (feats, frames) = fe.features(&audio);
        assert_eq!(frames, fe.n_frames(16_000));
        assert_eq!(feats.len(), frames * fe.n_mels());
        assert_eq!(fe.n_mels(), 80);
    }

    #[test]
    fn features_are_finite() {
        let fe = MelFrontend::new(MelConfig::default());
        let audio: Vec<f32> = (0..16_000)
            .map(|i| (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 16_000.0).sin() * 0.5)
            .collect();
        let (feats, _) = fe.features(&audio);
        assert!(feats.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn short_input_yields_no_frames() {
        let fe = MelFrontend::new(MelConfig::default());
        let (feats, frames) = fe.features(&[0.0; 100]);
        assert_eq!(frames, 0);
        assert!(feats.is_empty());
    }
}
