//! A single biquad section, used here as a high-pass to remove DC, rumble, and
//! handling noise before spectral processing. Coefficients per the RBJ audio
//! EQ cookbook (Direct Form I).

/// Transposed Direct Form II biquad.
#[derive(Clone)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    /// Identity (pass-through) filter.
    pub fn identity() -> Self {
        Self { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0, z1: 0.0, z2: 0.0 }
    }

    /// Second-order high-pass at `cutoff_hz` for the given `sample_rate`.
    /// `q` of ~0.707 gives a maximally-flat (Butterworth) response.
    pub fn highpass(sample_rate: u32, cutoff_hz: f32, q: f32) -> Self {
        if cutoff_hz <= 0.0 {
            return Self::identity();
        }
        let w0 = 2.0 * std::f32::consts::PI * cutoff_hz / sample_rate as f32;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);

        let b0 = (1.0 + cos) / 2.0;
        let b1 = -(1.0 + cos);
        let b2 = (1.0 + cos) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    pub fn process_sample(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }

    pub fn process(&mut self, buf: &mut [f32]) {
        for s in buf.iter_mut() {
            *s = self.process_sample(*s);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_passes_signal_unchanged() {
        let mut f = Biquad::identity();
        let mut buf = vec![0.5, -0.3, 0.9, -1.0];
        let copy = buf.clone();
        f.process(&mut buf);
        for (a, b) in buf.iter().zip(copy.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn highpass_attenuates_dc() {
        let mut f = Biquad::highpass(16_000, 200.0, 0.707);
        // Feed a DC offset; after settling the output should approach 0.
        let mut last = 0.0;
        for _ in 0..2000 {
            last = f.process_sample(1.0);
        }
        assert!(last.abs() < 0.05, "DC not removed: {last}");
    }

    #[test]
    fn highpass_preserves_high_freq_energy() {
        // Nyquist-ish alternating signal should largely pass a 200 Hz HPF.
        let mut f = Biquad::highpass(16_000, 200.0, 0.707);
        let mut energy = 0.0f32;
        for i in 0..2000 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 };
            let y = f.process_sample(x);
            if i > 100 {
                energy += y * y;
            }
        }
        assert!(energy > 100.0, "high freq wrongly attenuated: {energy}");
    }
}
