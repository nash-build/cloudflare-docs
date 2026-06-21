//! Spectral noise suppression.
//!
//! The built-in suppressor is a per-bin Wiener filter over a recursively
//! estimated noise power spectrum. The noise estimate is updated when the VAD
//! reports "no speech", so it tracks the room without eating the talker. This is
//! a real, shippable denoiser for stationary/quasi-stationary noise (fans,
//! traffic, hiss, hum).
//!
//! For non-stationary noise and tighter speech preservation, implement
//! [`NeuralDenoiser`] around DeepFilterNet (Rust, MIT/Apache) or another ONNX
//! model and pass it to the pipeline; the spectral suppressor stays as a
//! zero-dependency fallback and pre-cleaner.

/// A drop-in neural suppressor. Given a magnitude spectrum (first N/2+1 bins),
/// return a per-bin gain in `[0, 1]`. The pipeline multiplies the two stages,
/// so a neural model composes with the built-in estimator rather than replacing
/// the plumbing.
pub trait NeuralDenoiser: Send {
    fn gain(&mut self, magnitude: &[f32]) -> Vec<f32>;
}

/// Recursive-Wiener spectral suppressor.
pub struct SpectralDenoiser {
    noise_psd: Vec<f32>,
    /// Per-bin smoothed a-priori SNR (decision-directed), reduces musical noise.
    prev_snr: Vec<f32>,
    initialised: bool,
    /// Floor gain so suppressed bins keep a little signal (avoids gating
    /// artifacts / "underwater" sound). Scaled by strength.
    over_subtraction: f32,
}

impl SpectralDenoiser {
    pub fn new() -> Self {
        Self {
            noise_psd: Vec::new(),
            prev_snr: Vec::new(),
            initialised: false,
            over_subtraction: 1.5,
        }
    }

    fn ensure(&mut self, n: usize) {
        if self.noise_psd.len() != n {
            self.noise_psd = vec![1e-6; n];
            self.prev_snr = vec![1.0; n];
            self.initialised = false;
        }
    }

    /// Compute the suppression gain mask.
    ///
    /// * `mag`      — magnitude spectrum for this frame.
    /// * `is_noise` — VAD says this frame is noise-only (update the estimate).
    /// * `strength` — 0.0 (bypass) .. 1.0 (max suppression).
    pub fn gain(&mut self, mag: &[f32], is_noise: bool, strength: f32) -> Vec<f32> {
        let n = mag.len();
        self.ensure(n);

        if !self.initialised {
            for i in 0..n {
                self.noise_psd[i] = (mag[i] * mag[i]).max(1e-8);
            }
            self.initialised = true;
        } else if is_noise {
            // Track the noise floor only on noise frames.
            for i in 0..n {
                let p = mag[i] * mag[i];
                self.noise_psd[i] = 0.92 * self.noise_psd[i] + 0.08 * p;
            }
        }

        if strength <= 0.0 {
            return vec![1.0; n];
        }

        let floor = (1.0 - strength).max(0.02); // residual gain at full suppression
        let alpha = 0.98; // decision-directed smoothing
        let mut out = vec![0.0f32; n];
        for i in 0..n {
            let power = mag[i] * mag[i];
            let noise = self.noise_psd[i] * self.over_subtraction;
            // Maximum-likelihood a-posteriori SNR.
            let post = (power / (noise + 1e-9) - 1.0).max(0.0);
            // Decision-directed a-priori SNR (Ephraim–Malah style smoothing).
            let prior = alpha * self.prev_snr[i] + (1.0 - alpha) * post;
            self.prev_snr[i] = prior;
            // Wiener gain.
            let mut g = prior / (1.0 + prior);
            // Blend toward unity for gentle settings; clamp to the floor.
            g = floor + (1.0 - floor) * g;
            out[i] = g.clamp(floor, 1.0);
        }
        out
    }
}

impl Default for SpectralDenoiser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suppresses_steady_noise_bins() {
        let mut d = SpectralDenoiser::new();
        let n = 64;
        // Train on flat noise.
        let noise = vec![0.1f32; n];
        for _ in 0..50 {
            let _ = d.gain(&noise, true, 0.9);
        }
        // A frame that is just the same noise should get strong attenuation.
        let g = d.gain(&noise, false, 0.9);
        let avg: f32 = g.iter().sum::<f32>() / n as f32;
        assert!(avg < 0.5, "noise not suppressed, avg gain {avg}");
    }

    #[test]
    fn preserves_signal_above_noise() {
        let mut d = SpectralDenoiser::new();
        let n = 64;
        let noise = vec![0.1f32; n];
        for _ in 0..50 {
            let _ = d.gain(&noise, true, 0.9);
        }
        // A loud peak should keep most of its gain.
        let mut frame = noise.clone();
        frame[20] = 2.0;
        let g = d.gain(&frame, false, 0.9);
        assert!(g[20] > 0.8, "signal bin wrongly suppressed: {}", g[20]);
    }

    #[test]
    fn zero_strength_is_bypass() {
        let mut d = SpectralDenoiser::new();
        let g = d.gain(&vec![0.5; 32], false, 0.0);
        assert!(g.iter().all(|&x| (x - 1.0).abs() < 1e-6));
    }
}
