//! Sample-rate conversion to the engine's fixed 16 kHz working rate.
//!
//! Downsampling voice from 44.1/48 kHz with a band-limited linear interpolator
//! plus a one-pole anti-alias pre-filter is cheap, stable, and good enough at
//! the working rate; the neural front-ends downstream are robust to it. Swap for
//! `rubato` if you later need transparent quality.

use crate::config::OUTPUT_SAMPLE_RATE;

/// Streaming linear resampler from `input_rate` to [`OUTPUT_SAMPLE_RATE`].
///
/// State is carried across [`process`](Resampler::process) calls so chunk
/// boundaries are seamless (no clicks).
pub struct Resampler {
    input_rate: u32,
    /// in/out sample-rate ratio (how many input samples per output sample).
    step: f64,
    /// Fractional position of the next output sample within the current
    /// [prev, x] input interval, in input-sample units (0.0 == at `prev`).
    t: f64,
    prev: f32,
    primed: bool,
    // One-pole low-pass for light anti-aliasing when downsampling.
    lp: f32,
    lp_coeff: f32,
    passthrough: bool,
}

impl Resampler {
    pub fn new(input_rate: u32) -> Self {
        let step = input_rate as f64 / OUTPUT_SAMPLE_RATE as f64;
        let passthrough = input_rate == OUTPUT_SAMPLE_RATE;
        let lp_coeff = if input_rate > OUTPUT_SAMPLE_RATE {
            let fc = 0.45 * OUTPUT_SAMPLE_RATE as f32; // just below output Nyquist
            let dt = 1.0 / input_rate as f32;
            let rc = 1.0 / (2.0 * std::f32::consts::PI * fc);
            dt / (rc + dt)
        } else {
            1.0
        };
        Self {
            input_rate,
            step,
            t: 0.0,
            prev: 0.0,
            primed: false,
            lp: 0.0,
            lp_coeff,
            passthrough,
        }
    }

    pub fn input_rate(&self) -> u32 {
        self.input_rate
    }

    /// Resample one chunk, appending results to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.passthrough {
            out.extend_from_slice(input);
            return;
        }
        for &raw in input {
            let x = if self.lp_coeff < 1.0 {
                self.lp += self.lp_coeff * (raw - self.lp);
                self.lp
            } else {
                raw
            };

            if !self.primed {
                self.prev = x;
                self.primed = true;
                continue;
            }

            // Emit every output sample whose position falls in [prev, x).
            while self.t < 1.0 {
                let frac = self.t as f32;
                out.push(self.prev + (x - self.prev) * frac);
                self.t += self.step;
            }
            self.t -= 1.0;
            self.prev = x;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsample_3x_ratio_is_correct() {
        let mut r = Resampler::new(48_000);
        let mut out = Vec::new();
        // 3000 input samples @ 48k ≈ 1000 output samples @ 16k.
        let input: Vec<f32> = (0..3000).map(|i| (i as f32 * 0.01).sin()).collect();
        r.process(&input, &mut out);
        let expected = 3000 / 3;
        assert!((out.len() as i32 - expected as i32).abs() <= 2, "got {}", out.len());
    }

    #[test]
    fn passthrough_when_already_16k() {
        let mut r = Resampler::new(16_000);
        let mut out = Vec::new();
        let input = vec![0.1, 0.2, 0.3, 0.4];
        r.process(&input, &mut out);
        assert_eq!(out, input);
    }

    #[test]
    fn streaming_matches_single_shot_length() {
        let input: Vec<f32> = (0..4800).map(|i| (i as f32 * 0.02).sin()).collect();
        let mut a = Resampler::new(48_000);
        let mut one = Vec::new();
        a.process(&input, &mut one);

        let mut b = Resampler::new(48_000);
        let mut many = Vec::new();
        for chunk in input.chunks(137) {
            b.process(chunk, &mut many);
        }
        assert_eq!(one.len(), many.len());
    }
}
