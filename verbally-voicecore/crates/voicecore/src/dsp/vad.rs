//! A lightweight, per-frame voice activity detector.
//!
//! It combines two cues that are cheap to compute from the magnitude spectrum
//! the STFT already produces:
//!   * **energy** relative to an adaptively-tracked noise floor, and
//!   * **spectral flatness** — noise is flat/broadband, voiced speech is peaky.
//!
//! A hangover counter keeps the decision "on" briefly after speech stops so we
//! don't chop word tails. The noise floor only adapts while inactive, which is
//! what lets the denoiser learn the room.

/// Decision plus the diagnostic scores behind it (handy for tuning/telemetry).
#[derive(Debug, Clone, Copy)]
pub struct VadFrame {
    pub speech: bool,
    pub energy_db: f32,
    pub flatness: f32,
}

pub struct Vad {
    noise_energy: f32,
    hangover: u32,
    hangover_frames: u32,
    /// dB above the noise floor required to trip "speech".
    threshold_db: f32,
    initialised: bool,
}

impl Vad {
    pub fn new() -> Self {
        Self {
            noise_energy: 1e-6,
            hangover: 0,
            hangover_frames: 8, // ~128 ms at 16 ms hops
            threshold_db: 6.0,
            initialised: false,
        }
    }

    /// Analyse one magnitude frame (first FFT_SIZE/2+1 bins).
    pub fn analyse(&mut self, mag: &[f32]) -> VadFrame {
        let mut energy = 0.0f32;
        let mut log_sum = 0.0f32;
        let mut lin_sum = 0.0f32;
        let n = mag.len() as f32;
        for &m in mag {
            let p = m * m + 1e-12;
            energy += p;
            log_sum += p.ln();
            lin_sum += p;
        }
        // Spectral flatness = geometric mean / arithmetic mean of the power
        // spectrum, in [0,1]. ~1 == flat/noise, ~0 == tonal/voiced.
        let flatness = ((log_sum / n).exp()) / (lin_sum / n + 1e-12);

        if !self.initialised {
            self.noise_energy = energy.max(1e-6);
            self.initialised = true;
        }

        let energy_db = 10.0 * (energy / self.noise_energy).log10();
        // Voiced speech is both louder than the floor *and* not flat.
        let is_speech = energy_db > self.threshold_db && flatness < 0.5;

        if is_speech {
            self.hangover = self.hangover_frames;
        } else if self.hangover > 0 {
            self.hangover -= 1;
        } else {
            // Adapt the noise floor slowly while we're confident it's noise.
            self.noise_energy = 0.95 * self.noise_energy + 0.05 * energy;
        }

        VadFrame {
            speech: is_speech || self.hangover > 0,
            energy_db,
            flatness,
        }
    }
}

impl Default for Vad {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_noise(level: f32, n: usize) -> Vec<f32> {
        // pseudo-random but deterministic flat-ish spectrum
        (0..n).map(|k| level * (1.0 + 0.1 * ((k * 7 % 5) as f32))).collect()
    }

    fn tonal(level: f32, n: usize) -> Vec<f32> {
        // energy concentrated in a few bins
        let mut v = vec![0.001 * level; n];
        for k in [10, 11, 20, 40] {
            if k < n {
                v[k] = level;
            }
        }
        v
    }

    #[test]
    fn quiet_flat_noise_is_not_speech() {
        let mut vad = Vad::new();
        let mut any_speech = false;
        for _ in 0..50 {
            any_speech |= vad.analyse(&flat_noise(0.01, 257)).speech;
        }
        assert!(!any_speech);
    }

    #[test]
    fn loud_tonal_burst_is_speech() {
        let mut vad = Vad::new();
        for _ in 0..30 {
            vad.analyse(&flat_noise(0.01, 257)); // establish floor
        }
        let f = vad.analyse(&tonal(2.0, 257));
        assert!(f.speech, "energy_db={} flatness={}", f.energy_db, f.flatness);
    }
}
