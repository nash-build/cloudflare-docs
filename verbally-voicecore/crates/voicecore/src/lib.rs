//! # voicecore
//!
//! A clean-room, real-time **near-field voice isolation** engine for a single
//! microphone. It keeps the *enrolled* speaker, attenuates other voices and
//! ambient noise, and emits 16 kHz mono PCM ready to stream into an ElevenLabs
//! speech-to-speech / agent session.
//!
//! It is written from first principles using open DSP/ML techniques and
//! permissively-licensed building blocks; it contains no third-party SDK code.
//!
//! ## Quick start
//! ```
//! use voicecore::{Config, VoiceEngine};
//!
//! let mut cfg = Config::default();
//! cfg.input_sample_rate = 48_000; // your mic's rate
//! let mut engine = VoiceEngine::new(cfg).unwrap();
//!
//! // 1) Enroll: feed ~20–30 s of just your voice.
//! engine.begin_enrollment();
//! # let your_voice = vec![0.0f32; 48_000];
//! let _ = engine.process(&your_voice);
//! let profile = engine.finish_enrollment().unwrap();
//! let saved: Vec<u8> = profile.to_bytes(); // persist to disk/keychain
//!
//! // 2) Run: clean audio in → isolated 16 kHz mono out → ElevenLabs.
//! # let mic_chunk = vec![0.0f32; 480];
//! let clean = engine.process(&mic_chunk);
//! # let _ = (saved, clean);
//! ```
//!
//! ## Extending with neural models
//! The DSP path is the always-available baseline. Drop in stronger models via
//! traits without touching the plumbing:
//! * [`dsp::denoise::NeuralDenoiser`] — e.g. DeepFilterNet (Rust, MIT/Apache).
//! * [`speaker::embedding::Embedder`] — e.g. ECAPA-TDNN via the `ort` crate.

pub mod config;
pub mod dsp;
pub mod pipeline;
pub mod resample;
pub mod speaker;

pub use config::{Config, ConfigError, OUTPUT_SAMPLE_RATE};
pub use pipeline::Pipeline;
pub use speaker::embedding::SpeakerProfile;

/// Library version (from Cargo).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// High-level engine: a validated [`Config`] wrapped around the [`Pipeline`].
///
/// This is the type native shells bind to (directly or through the FFI crate).
pub struct VoiceEngine {
    pipeline: Pipeline,
}

impl VoiceEngine {
    /// Create an engine, validating the configuration.
    pub fn new(cfg: Config) -> Result<Self, ConfigError> {
        cfg.validate()?;
        Ok(Self { pipeline: Pipeline::new(cfg) })
    }

    /// Process a chunk of input audio (at the configured input rate) and return
    /// cleaned 16 kHz mono samples.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        self.pipeline.process(input)
    }

    /// Begin capturing an enrollment clip (feed ~20–30 s of only your voice).
    pub fn begin_enrollment(&mut self) {
        self.pipeline.begin_enrollment();
    }

    /// Enrollment frames captured so far.
    pub fn enrollment_frames(&self) -> u64 {
        self.pipeline.enrollment_frames()
    }

    /// Finish enrollment and install the profile; returns it for persistence.
    pub fn finish_enrollment(&mut self) -> Option<SpeakerProfile> {
        self.pipeline.finish_enrollment()
    }

    /// Install a previously saved profile (e.g. loaded from disk at startup).
    pub fn set_profile(&mut self, profile: &SpeakerProfile) {
        self.pipeline.set_profile(profile);
    }

    /// Whether a speaker profile is currently active.
    pub fn is_enrolled(&self) -> bool {
        self.pipeline.is_enrolled()
    }

    /// Feed an external speaker-presence score in `[0, 1]` (e.g. from an ECAPA
    /// rolling re-scorer) to reinforce the per-frame speaker gate.
    pub fn set_speaker_presence(&mut self, score: f32) {
        self.pipeline.set_speaker_presence(score);
    }

    /// Access the underlying pipeline (to install neural models, etc.).
    pub fn pipeline_mut(&mut self) -> &mut Pipeline {
        &mut self.pipeline
    }
}

// --- Profile serialisation -------------------------------------------------

impl SpeakerProfile {
    /// Serialise the centroid to a compact little-endian byte blob:
    /// `[u32 dim][f32 * dim]`.
    pub fn to_bytes(&self) -> Vec<u8> {
        let c = self.centroid();
        let mut out = Vec::with_capacity(4 + c.len() * 4);
        out.extend_from_slice(&(c.len() as u32).to_le_bytes());
        for v in c {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out
    }

    /// Restore a profile previously produced by [`to_bytes`](Self::to_bytes).
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 4 {
            return None;
        }
        let dim = u32::from_le_bytes(bytes[0..4].try_into().ok()?) as usize;
        if bytes.len() != 4 + dim * 4 {
            return None;
        }
        let mut centroid = Vec::with_capacity(dim);
        for i in 0..dim {
            let o = 4 + i * 4;
            centroid.push(f32::from_le_bytes(bytes[o..o + 4].try_into().ok()?));
        }
        Some(SpeakerProfile::from_centroid(centroid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_config() {
        let mut cfg = Config::default();
        cfg.input_sample_rate = 0;
        assert!(VoiceEngine::new(cfg).is_err());
    }

    #[test]
    fn profile_round_trips_through_bytes() {
        use crate::speaker::embedding::{BandEmbedder, Embedder};
        let emb = BandEmbedder::new(257);
        let mut p = SpeakerProfile::new(emb.dim());
        let mut m = vec![0.02f32; 257];
        for k in 5..25 {
            m[k] = 1.0;
        }
        for _ in 0..10 {
            p.add(&emb.embed(&m));
        }
        let bytes = p.to_bytes();
        let restored = SpeakerProfile::from_bytes(&bytes).unwrap();
        let a = p.centroid();
        let b = restored.centroid();
        for (x, y) in a.iter().zip(b.iter()) {
            assert!((x - y).abs() < 1e-6);
        }
    }

    #[test]
    fn from_bytes_rejects_garbage() {
        assert!(SpeakerProfile::from_bytes(&[1, 2, 3]).is_none());
    }
}
