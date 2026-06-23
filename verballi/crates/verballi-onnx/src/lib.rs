//! ONNX adapters for `verballi`.
//!
//! * [`features`] — a pure-Rust **log-mel front-end** for speaker models. Always
//!   available; no ONNX Runtime required.
//! * `ecapa` (feature `ort`) — an **ECAPA-TDNN** [`SpeakerVerifier`] backed by
//!   ONNX Runtime. Drop it into `verballi`'s `PresenceScorer` for a
//!   trained-model identity lock.
//! * `enhance` (feature `ort`) — an ONNX **time-domain enhancer** (e.g. a DTLN /
//!   DeepFilterNet export) implementing `verballi`'s `Enhancer`.
//!
//! The `ort` feature is opt-in so the crate and its mel front-end build
//! everywhere; enable it on a host where ONNX Runtime is available and supply
//! your own model exports.
//!
//! ```ignore
//! // Cargo.toml: verballi-onnx = { path = "...", features = ["ort"] }
//! use verballi_onnx::ecapa::OnnxEcapaVerifier;
//! use verballi::speaker::verify::PresenceScorer;
//!
//! let verifier = OnnxEcapaVerifier::from_file("ecapa.onnx")?;
//! let mut scorer = PresenceScorer::new(Box::new(verifier), 1.0, 0.25);
//! scorer.set_reference_audio(&enrollment_16k);
//! ```

pub mod features;
pub use features::{MelConfig, MelFrontend};

#[cfg(feature = "ort")]
pub mod ecapa;
#[cfg(feature = "ort")]
pub mod enhance;

/// L2-normalise a vector in place (used by the ONNX adapters and handy for
/// callers post-processing model embeddings).
pub fn l2_normalise(v: &mut [f32]) {
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if n > 1e-9 {
        for x in v.iter_mut() {
            *x /= n;
        }
    }
}
