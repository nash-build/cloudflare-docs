//! Digital signal-processing building blocks for the engine.
//!
//! Each module is independent and unit-tested; [`crate::pipeline`] wires them
//! into the real-time graph.

pub mod agc;
pub mod biquad;
pub mod denoise;
pub mod enhance;
pub mod stft;
pub mod vad;
