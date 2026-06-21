//! The real-time processing graph.
//!
//! ```text
//! in → resample(16k) → high-pass → STFT ─┬─ VAD ──────────────┐
//!                                         ├─ denoise mask ──────┤ per-bin × scalar
//!                                         └─ speaker gate (scalar) ┘ → ISTFT → AGC/limiter → out
//! ```
//!
//! Everything is streaming and allocation-light on the hot path; you can feed
//! arbitrary chunk sizes and pull whatever comes out.

use crate::config::{Config, FFT_SIZE, OUTPUT_SAMPLE_RATE};
use crate::dsp::agc::Agc;
use crate::dsp::biquad::Biquad;
use crate::dsp::denoise::{NeuralDenoiser, SpectralDenoiser};
use crate::dsp::stft::Stft;
use crate::dsp::vad::Vad;
use crate::resample::Resampler;
use crate::speaker::embedding::{BandEmbedder, Embedder, SpeakerProfile};
use crate::speaker::gate::SpeakerGate;

const N_BINS: usize = FFT_SIZE / 2 + 1;
const HOP: usize = FFT_SIZE / 2;

pub struct Pipeline {
    cfg: Config,
    resampler: Resampler,
    hpf: Biquad,
    stft: Stft,
    vad: Vad,
    denoise: SpectralDenoiser,
    neural: Option<Box<dyn NeuralDenoiser>>,
    embedder: Box<dyn Embedder>,
    gate: SpeakerGate,
    agc: Agc,

    // scratch buffers reused across calls
    resampled: Vec<f32>,
    // when Some, we are enrolling: accumulate embeddings, pass audio unchanged
    enrolling: Option<SpeakerProfile>,
}

impl Pipeline {
    pub fn new(cfg: Config) -> Self {
        let embedder = Box::new(BandEmbedder::new(N_BINS));
        let dim = embedder.dim();
        let mut hpf = Biquad::highpass(OUTPUT_SAMPLE_RATE, cfg.highpass_hz, 0.707);
        if cfg.highpass_hz <= 0.0 {
            hpf = Biquad::identity();
        }
        Self {
            resampler: Resampler::new(cfg.input_sample_rate),
            hpf,
            stft: Stft::new(),
            vad: Vad::new(),
            denoise: SpectralDenoiser::new(),
            neural: None,
            embedder,
            gate: SpeakerGate::new(dim),
            agc: Agc::new(cfg.agc_target_dbfs, cfg.agc_max_gain_db),
            resampled: Vec::with_capacity(2048),
            enrolling: None,
            cfg,
        }
    }

    /// Install a neural denoiser (e.g. DeepFilterNet). Composes with the
    /// built-in spectral suppressor.
    pub fn set_neural_denoiser(&mut self, d: Box<dyn NeuralDenoiser>) {
        self.neural = Some(d);
    }

    /// Install a custom embedder (e.g. ECAPA-TDNN ONNX). Resets the gate.
    pub fn set_embedder(&mut self, e: Box<dyn Embedder>) {
        let dim = e.dim();
        self.embedder = e;
        self.gate = SpeakerGate::new(dim);
    }

    /// Install an enrolled speaker profile so the gate keeps only that voice.
    pub fn set_profile(&mut self, profile: &SpeakerProfile) {
        self.gate.set_profile(profile);
    }

    pub fn is_enrolled(&self) -> bool {
        self.gate.is_enrolled()
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// Begin enrollment: subsequent [`process`](Pipeline::process) calls return
    /// the audio essentially untouched while building the voice fingerprint.
    pub fn begin_enrollment(&mut self) {
        self.enrolling = Some(SpeakerProfile::new(self.embedder.dim()));
    }

    /// How many frames of enrollment captured so far (0 if not enrolling).
    pub fn enrollment_frames(&self) -> u64 {
        self.enrolling.as_ref().map(|p| p.frames()).unwrap_or(0)
    }

    /// Finish enrollment, install the profile on the gate, and return it so the
    /// caller can persist it.
    pub fn finish_enrollment(&mut self) -> Option<SpeakerProfile> {
        let profile = self.enrolling.take()?;
        self.gate.set_profile(&profile);
        Some(profile)
    }

    /// Process a chunk of input audio (at `cfg.input_sample_rate`) and return
    /// cleaned 16 kHz mono samples. Output length varies with rate conversion
    /// and STFT buffering; just append/stream whatever is returned.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        self.resampled.clear();
        self.resampler.process(input, &mut self.resampled);
        self.hpf.process(&mut self.resampled);

        // Disjoint field borrows so the STFT closure can touch the analysers
        // without re-borrowing `self`.
        let Self {
            cfg,
            stft,
            vad,
            denoise,
            neural,
            embedder,
            gate,
            enrolling,
            resampled,
            ..
        } = self;
        let embedder: &dyn Embedder = embedder.as_ref();

        let mut out = Vec::with_capacity(resampled.len());
        // active flag per emitted hop, for the AGC pass below
        let mut active: Vec<bool> = Vec::new();

        stft.process(resampled, &mut out, |mag| {
            let v = vad.analyse(mag);

            // Suppression mask (built-in Wiener), optionally × neural mask.
            let mut mask = denoise.gain(mag, !v.speech, cfg.denoise_strength);
            if let Some(nd) = neural.as_deref_mut() {
                let ng = nd.gain(mag);
                for (m, g) in mask.iter_mut().zip(ng.iter()) {
                    *m *= *g;
                }
            }

            // Speaker / proximity scalar (skipped while enrolling).
            let spk = if let Some(profile) = enrolling.as_mut() {
                profile.add(&embedder.embed(mag));
                1.0
            } else {
                gate.gain(
                    embedder,
                    mag,
                    cfg.speaker_focus,
                    cfg.proximity_focus,
                    cfg.speaker_gate_floor,
                )
            };

            for m in mask.iter_mut() {
                *m *= spk;
            }
            active.push(v.speech && spk > 0.5);
            mask
        });

        // AGC + limiter, hop-aligned with the activity decisions.
        let agc = &mut self.agc;
        for (i, chunk) in out.chunks_mut(HOP).enumerate() {
            let act = active.get(i).copied().unwrap_or(false);
            agc.process(chunk, act);
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(freq: f32, n: usize, rate: f32, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (2.0 * std::f32::consts::PI * freq * i as f32 / rate).sin())
            .collect()
    }

    #[test]
    fn passthrough_produces_output_at_16k() {
        let mut cfg = Config::default();
        cfg.input_sample_rate = 48_000;
        cfg.denoise_strength = 0.0;
        cfg.speaker_focus = 0.0;
        cfg.proximity_focus = 0.0;
        let mut p = Pipeline::new(cfg);
        let input = tone(440.0, 48_000, 48_000.0, 0.3); // 1 s
        let out = p.process(&input);
        // ~1 s at 16 kHz, minus STFT priming latency.
        assert!(out.len() > 15_000 && out.len() <= 16_000, "len {}", out.len());
    }

    #[test]
    fn denoise_reduces_broadband_noise_energy() {
        let mut cfg = Config::default();
        cfg.input_sample_rate = 16_000;
        cfg.denoise_strength = 0.9;
        cfg.speaker_focus = 0.0;
        cfg.proximity_focus = 0.0;
        let mut p = Pipeline::new(cfg);

        // deterministic pseudo-noise
        let mut seed = 12345u32;
        let noise: Vec<f32> = (0..32_000)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                ((seed >> 9) as f32 / (1 << 23) as f32 - 1.0) * 0.2
            })
            .collect();
        let out = p.process(&noise);
        let in_e: f32 = noise.iter().map(|x| x * x).sum::<f32>() / noise.len() as f32;
        let out_e: f32 = out.iter().map(|x| x * x).sum::<f32>() / out.len().max(1) as f32;
        assert!(out_e < in_e, "noise energy not reduced: in {in_e} out {out_e}");
    }

    #[test]
    fn enrollment_then_gate_runs_end_to_end() {
        let mut cfg = Config::default();
        cfg.input_sample_rate = 16_000;
        let mut p = Pipeline::new(cfg);

        p.begin_enrollment();
        let me = tone(180.0, 32_000, 16_000.0, 0.4);
        let _ = p.process(&me);
        assert!(p.enrollment_frames() > 0);
        let profile = p.finish_enrollment().expect("profile");
        assert!(profile.frames() > 0);
        assert!(p.is_enrolled());

        // Now process again; should produce output without panicking.
        let out = p.process(&me);
        assert!(!out.is_empty());
    }
}
