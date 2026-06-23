//! Short-time Fourier transform with a square-root Hann window and 50%
//! overlap-add (WOLA).
//!
//! Using a *sqrt-Hann* window for both analysis and synthesis makes the product
//! of the two windows a full Hann window, which sums to a constant at 50% hop —
//! the constant-overlap-add condition. So analysis→(spectral gain)→synthesis
//! reconstructs the input exactly when the gain is unity, and the per-bin gain
//! mask the engine applies in between is artifact-light.

use crate::config::{FFT_SIZE, HOP_SIZE};
use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::sync::Arc;

/// Precomputed periodic sqrt-Hann window of length [`FFT_SIZE`]. Applied on both
/// analysis and synthesis so the effective window is a full Hann.
fn sqrt_hann() -> [f32; FFT_SIZE] {
    let mut w = [0.0f32; FFT_SIZE];
    for (n, wn) in w.iter_mut().enumerate() {
        let hann = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * n as f32 / FFT_SIZE as f32).cos();
        *wn = hann.sqrt();
    }
    w
}

/// Streaming STFT processor. Feed it arbitrary-length audio; it buffers into
/// [`FFT_SIZE`] frames hopped by [`HOP_SIZE`], hands each magnitude/phase frame
/// to your closure, and overlap-adds the result back to a continuous stream.
pub struct Stft {
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    window: [f32; FFT_SIZE],
    // Normalisation so WOLA reconstructs unity: for Hann @ 50% with rustfft's
    // unnormalised inverse, divide by FFT_SIZE and by the window-overlap sum.
    norm: f32,
    in_buf: Vec<f32>,   // accumulates input until we have a full frame
    out_buf: Vec<f32>,  // overlap-add accumulator, drained in hops
    scratch: Vec<Complex32>,
}

impl Stft {
    pub fn new() -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        let ifft = planner.plan_fft_inverse(FFT_SIZE);
        // rustfft's inverse is unnormalised (ifft(fft(x)) = N·x). The sqrt-Hann
        // analysis×synthesis product sums to 1 across the 50% overlap, so the
        // only scale to undo is the FFT's N.
        let norm = 1.0 / FFT_SIZE as f32;
        Self {
            fft,
            ifft,
            window: sqrt_hann(),
            norm,
            in_buf: Vec::with_capacity(FFT_SIZE * 2),
            out_buf: vec![0.0; FFT_SIZE],
            scratch: vec![Complex32::new(0.0, 0.0); FFT_SIZE],
        }
    }

    /// Push input; for every completed hop, `gain` is called with the magnitude
    /// spectrum (first FFT_SIZE/2+1 bins) and must return a per-bin gain in the
    /// same layout. Reconstructed audio is appended to `out`.
    pub fn process<F>(&mut self, input: &[f32], out: &mut Vec<f32>, mut gain: F)
    where
        F: FnMut(&[f32]) -> Vec<f32>,
    {
        self.in_buf.extend_from_slice(input);

        while self.in_buf.len() >= FFT_SIZE {
            // Window the oldest FFT_SIZE samples into the complex scratch.
            for i in 0..FFT_SIZE {
                self.scratch[i] = Complex32::new(self.in_buf[i] * self.window[i], 0.0);
            }
            self.fft.process(&mut self.scratch);

            // Magnitude of the non-redundant half.
            let half = FFT_SIZE / 2 + 1;
            let mut mag = vec![0.0f32; half];
            for (k, m) in mag.iter_mut().enumerate() {
                *m = self.scratch[k].norm();
            }

            // Caller computes per-bin gain from the magnitude spectrum.
            let g = gain(&mag);
            debug_assert_eq!(g.len(), half);

            // Apply gain symmetrically across the full spectrum.
            for k in 0..half {
                self.scratch[k] *= g[k];
            }
            for k in 1..(FFT_SIZE / 2) {
                self.scratch[FFT_SIZE - k] = self.scratch[k].conj();
            }

            self.ifft.process(&mut self.scratch);

            // Window again (synthesis) and overlap-add.
            for i in 0..FFT_SIZE {
                self.out_buf[i] += self.scratch[i].re * self.window[i] * self.norm;
            }

            // The oldest HOP_SIZE output samples are now final.
            out.extend_from_slice(&self.out_buf[..HOP_SIZE]);
            self.out_buf.copy_within(HOP_SIZE.., 0);
            let tail = self.out_buf.len() - HOP_SIZE;
            for v in &mut self.out_buf[tail..] {
                *v = 0.0;
            }

            // Drop the consumed hop from the input buffer.
            self.in_buf.drain(..HOP_SIZE);
        }
    }
}

impl Default for Stft {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unity_gain_reconstructs_signal() {
        let mut stft = Stft::new();
        let n = FFT_SIZE * 8;
        let input: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16_000.0).sin() * 0.5)
            .collect();
        let mut out = Vec::new();
        stft.process(&input, &mut out, |mag| vec![1.0; mag.len()]);

        // Output aligns with input; skip the first frame as a WOLA startup
        // transient and compare the steady-state region index-for-index.
        let lat = FFT_SIZE;
        let compare = 8_000usize.min(out.len().saturating_sub(lat));
        let mut err = 0.0f32;
        let mut ref_energy = 0.0f32;
        for i in 0..compare {
            let a = out[lat + i];
            let b = input[lat + i];
            err += (a - b) * (a - b);
            ref_energy += b * b;
        }
        let rel = (err / ref_energy).sqrt();
        assert!(rel < 0.02, "reconstruction error too high: {rel}");
    }
}
