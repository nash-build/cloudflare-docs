//! Time-domain enhancement seam.
//!
//! Some state-of-the-art denoisers — notably **DeepFilterNet** — are *not*
//! per-bin masks over our STFT. They run their own internal analysis (ERB bands,
//! deep filtering, look-ahead) and take time-domain audio in and give cleaned
//! time-domain audio out. This trait is the correct place to host them: it sits
//! right after resampling/high-pass and before our STFT stage, so a neural
//! enhancer and our lightweight spectral suppressor compose cleanly (enhancer
//! first, our Wiener mask as a gentle finisher — or turn `denoise_strength` down
//! and let the model do the work).
//!
//! Input and output are 16 kHz mono. Implementations may buffer internally and
//! introduce their own latency; returning fewer/more samples than were passed is
//! fine (the downstream STFT is fully streaming).

/// A streaming time-domain speech enhancer.
pub trait Enhancer: Send {
    /// Process 16 kHz mono samples, returning cleaned 16 kHz mono samples.
    fn process(&mut self, input: &[f32]) -> Vec<f32>;
}

/// Wraps a closure as an [`Enhancer`] — handy for plugging a model runtime
/// (DeepFilterNet via `tract`/`ort`, etc.) without a dedicated type.
pub struct ClosureEnhancer<F: FnMut(&[f32]) -> Vec<f32> + Send> {
    f: F,
}

impl<F: FnMut(&[f32]) -> Vec<f32> + Send> ClosureEnhancer<F> {
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

impl<F: FnMut(&[f32]) -> Vec<f32> + Send> Enhancer for ClosureEnhancer<F> {
    fn process(&mut self, input: &[f32]) -> Vec<f32> {
        (self.f)(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closure_enhancer_runs() {
        let mut e = ClosureEnhancer::new(|x: &[f32]| x.iter().map(|s| s * 0.5).collect());
        let out = e.process(&[1.0, 2.0, 3.0]);
        assert_eq!(out, vec![0.5, 1.0, 1.5]);
    }
}
