//! Automatic gain control with a brick-wall limiter.
//!
//! After suppression the level can vary a lot (we just removed energy and the
//! talker moves relative to the mic). The AGC tracks short-term RMS and nudges
//! the gain toward a target dBFS, but only while there is real signal, so it
//! never pumps the noise floor up between words. A fast look-free limiter keeps
//! peaks below 0 dBFS so nothing clips on the way into ElevenLabs.

pub struct Agc {
    gain: f32,
    target_rms: f32,
    max_gain: f32,
    rms: f32,
    // smoothing for the rms tracker
    rms_coeff: f32,
    // how fast the gain itself moves
    attack: f32,
    release: f32,
}

impl Agc {
    /// `target_dbfs` e.g. -18.0; `max_gain_db` caps upward gain e.g. 18.0.
    pub fn new(target_dbfs: f32, max_gain_db: f32) -> Self {
        Self {
            gain: 1.0,
            target_rms: db_to_lin(target_dbfs),
            max_gain: db_to_lin(max_gain_db),
            rms: 0.0,
            rms_coeff: 0.05,
            attack: 0.2,
            release: 0.02,
        }
    }

    /// Apply AGC + limiting in place. `active` should be true when the frame
    /// contains the wanted speaker (don't ride gain on silence/noise).
    pub fn process(&mut self, buf: &mut [f32], active: bool) {
        // Update short-term RMS.
        let mut sumsq = 0.0f32;
        for &s in buf.iter() {
            sumsq += s * s;
        }
        let frame_rms = (sumsq / buf.len().max(1) as f32).sqrt();
        self.rms += self.rms_coeff * (frame_rms - self.rms);

        if active && self.rms > 1e-4 {
            let desired = (self.target_rms / self.rms).clamp(1e-3, self.max_gain);
            // Move faster when we need to pull down (attack) than up (release).
            let coeff = if desired < self.gain { self.attack } else { self.release };
            self.gain += coeff * (desired - self.gain);
        }

        for s in buf.iter_mut() {
            let mut y = *s * self.gain;
            // Soft brick-wall limiter at -0.5 dBFS.
            const CEIL: f32 = 0.944; // ~ -0.5 dBFS
            if y > CEIL {
                y = CEIL + (y - CEIL).tanh() * (1.0 - CEIL);
            } else if y < -CEIL {
                y = -CEIL + (y + CEIL).tanh() * (1.0 - CEIL);
            }
            *s = y;
        }
    }
}

fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boosts_quiet_speech_toward_target() {
        let mut agc = Agc::new(-18.0, 24.0);
        let target = db_to_lin(-18.0);
        // Quiet steady tone well below target.
        let mut last_rms = 0.0;
        for _ in 0..400 {
            let mut buf: Vec<f32> = (0..256)
                .map(|i| 0.02 * (i as f32 * 0.3).sin())
                .collect();
            agc.process(&mut buf, true);
            let s: f32 = buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32;
            last_rms = s.sqrt();
        }
        // Should have risen substantially toward the target.
        assert!(last_rms > target * 0.4, "rms {last_rms} target {target}");
    }

    #[test]
    fn never_exceeds_ceiling() {
        let mut agc = Agc::new(-6.0, 24.0);
        for _ in 0..200 {
            let mut buf = vec![5.0f32; 128]; // absurdly hot input
            agc.process(&mut buf, true);
            assert!(buf.iter().all(|&x| x.abs() <= 1.0), "limiter let a peak through");
        }
    }

    #[test]
    fn holds_gain_when_inactive() {
        let mut agc = Agc::new(-18.0, 24.0);
        let g0 = agc.gain;
        for _ in 0..100 {
            let mut buf = vec![0.0001f32; 128];
            agc.process(&mut buf, false);
        }
        assert!((agc.gain - g0).abs() < 1e-3, "gain drifted on silence");
    }
}
