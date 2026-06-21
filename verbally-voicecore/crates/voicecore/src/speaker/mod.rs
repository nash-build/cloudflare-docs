//! Target-speaker extraction: enroll the wanted voice, then keep only it.
//!
//! * [`embedding`] — turn audio into a voice fingerprint (built-in band
//!   descriptor today; ECAPA-TDNN ONNX drop-in tomorrow).
//! * [`proximity`] — single-mic "closest to the mic" estimator.
//! * [`gate`]      — combine identity + proximity into a per-frame gain.

pub mod embedding;
pub mod gate;
pub mod proximity;

pub use embedding::{cosine, BandEmbedder, Embedder, SpeakerProfile};
pub use gate::SpeakerGate;
pub use proximity::Proximity;
