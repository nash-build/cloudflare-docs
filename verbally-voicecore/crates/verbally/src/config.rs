//! Engine configuration.
//!
//! The defaults are tuned for conversational use with an ElevenLabs agent:
//! 16 kHz mono output, 32 ms analysis windows with 50% overlap, and a
//! denoiser/speaker-gate balance that favours intelligibility over maximum
//! suppression (over-suppression hurts EL's turn-taking / VAD).

/// Output sample rate the engine always emits, regardless of capture rate.
/// 16 kHz mono is what ElevenLabs STS / agent streams expect.
pub const OUTPUT_SAMPLE_RATE: u32 = 16_000;

/// STFT frame size in samples (at [`OUTPUT_SAMPLE_RATE`]). 512 → 32 ms.
pub const FFT_SIZE: usize = 512;

/// Hop size (50% overlap). 256 → 16 ms of new audio per processed frame.
pub const HOP_SIZE: usize = FFT_SIZE / 2;

/// Tuning knobs for the processing graph. All fields are safe to change at
/// construction time; see [`Config::validate`] for accepted ranges.
#[derive(Debug, Clone)]
pub struct Config {
    /// Sample rate of the audio you feed in (e.g. 48000 from a laptop mic).
    pub input_sample_rate: u32,

    /// High-pass cutoff (Hz) to kill rumble/handling/DC. 0 disables it.
    pub highpass_hz: f32,

    /// Noise-suppression aggressiveness, 0.0 (off) .. 1.0 (max).
    pub denoise_strength: f32,

    /// How strongly to attenuate audio that does not match the enrolled
    /// speaker, 0.0 (let everything through) .. 1.0 (hard reject others).
    pub speaker_focus: f32,

    /// Minimum gain (linear) ever applied by the speaker gate, so a brief
    /// mismatch never produces a fully dead, "dropped-call" silence.
    pub speaker_gate_floor: f32,

    /// Reject sources that are far/roomy using the direct-to-reverberant
    /// proxy, 0.0 (off) .. 1.0. Secondary to the speaker gate.
    pub proximity_focus: f32,

    /// Target output level (dBFS RMS) for the AGC. Typical: -18.0.
    pub agc_target_dbfs: f32,

    /// Max gain (dB) the AGC may apply, to avoid blowing up the noise floor.
    pub agc_max_gain_db: f32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            input_sample_rate: 48_000,
            highpass_hz: 80.0,
            denoise_strength: 0.75,
            speaker_focus: 0.8,
            speaker_gate_floor: 0.05,
            proximity_focus: 0.4,
            agc_target_dbfs: -18.0,
            agc_max_gain_db: 18.0,
        }
    }
}

/// Reasons a [`Config`] was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// `input_sample_rate` was 0 or implausibly high (> 384 kHz).
    BadSampleRate,
    /// A 0.0..=1.0 knob was outside that range.
    OutOfRange(&'static str),
}

impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ConfigError::BadSampleRate => write!(f, "input_sample_rate must be 1..=384000"),
            ConfigError::OutOfRange(name) => write!(f, "{name} must be within 0.0..=1.0"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    /// Validate the configuration, clamping is *not* performed — invalid input
    /// is reported so callers can surface a clear error.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.input_sample_rate == 0 || self.input_sample_rate > 384_000 {
            return Err(ConfigError::BadSampleRate);
        }
        let unit = [
            ("denoise_strength", self.denoise_strength),
            ("speaker_focus", self.speaker_focus),
            ("speaker_gate_floor", self.speaker_gate_floor),
            ("proximity_focus", self.proximity_focus),
        ];
        for (name, v) in unit {
            if !(0.0..=1.0).contains(&v) {
                return Err(ConfigError::OutOfRange(name));
            }
        }
        Ok(())
    }
}
