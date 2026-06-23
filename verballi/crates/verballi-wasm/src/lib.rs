//! WebAssembly bindings for `verballi`.
//!
//! Build with `wasm-pack build --target web` (or `cargo build --target
//! wasm32-unknown-unknown` + `wasm-bindgen`), then drive it from an
//! **AudioWorklet**: feed each render quantum of mic audio to [`WasmEngine::process`]
//! and pass the cleaned 16 kHz mono output to the ElevenLabs JS SDK.
//!
//! ```js
//! import init, { WasmEngine } from "./pkg/verballi_wasm.js";
//! await init();
//! const engine = new WasmEngine(48000);     // mic sample rate
//! // enrollment (once): feed ~20–30s of your voice, then:
//! const profileBytes = engine.finish_enrollment();   // Uint8Array → persist
//! // per audio frame:
//! const clean = engine.process(micFloat32);          // Float32Array @ 16 kHz
//! ```

use verballi::{Config, SpeakerProfile, VoiceEngine};
use wasm_bindgen::prelude::*;

/// Voice-isolation engine exposed to JavaScript.
#[wasm_bindgen]
pub struct WasmEngine {
    inner: VoiceEngine,
}

#[wasm_bindgen]
impl WasmEngine {
    /// Create an engine for the given mic sample rate (Hz). Throws on invalid
    /// configuration.
    #[wasm_bindgen(constructor)]
    pub fn new(input_sample_rate: u32) -> Result<WasmEngine, JsValue> {
        let mut cfg = Config::default();
        cfg.input_sample_rate = input_sample_rate;
        VoiceEngine::new(cfg)
            .map(|inner| WasmEngine { inner })
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }

    /// Process input mic samples → cleaned 16 kHz mono (as a `Float32Array`).
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        self.inner.process(input)
    }

    /// Begin enrollment; feed your voice via [`process`](Self::process).
    pub fn begin_enrollment(&mut self) {
        self.inner.begin_enrollment();
    }

    /// Enrollment frames captured so far.
    pub fn enrollment_frames(&self) -> u32 {
        self.inner.enrollment_frames() as u32
    }

    /// Finish enrollment; returns the serialised profile (`Uint8Array`), empty
    /// if not enrolling.
    pub fn finish_enrollment(&mut self) -> Vec<u8> {
        self.inner
            .finish_enrollment()
            .map(|p| p.to_bytes())
            .unwrap_or_default()
    }

    /// Install a previously saved profile; returns `false` if the bytes are
    /// invalid.
    pub fn set_profile(&mut self, bytes: &[u8]) -> bool {
        match SpeakerProfile::from_bytes(bytes) {
            Some(p) => {
                self.inner.set_profile(&p);
                true
            }
            None => false,
        }
    }

    /// Whether a profile is active.
    pub fn is_enrolled(&self) -> bool {
        self.inner.is_enrolled()
    }

    /// Feed an external (e.g. ECAPA) presence score in `[0, 1]`.
    pub fn set_speaker_presence(&mut self, score: f32) {
        self.inner.set_speaker_presence(score);
    }
}

/// The engine's fixed output sample rate (16000).
#[wasm_bindgen]
pub fn output_sample_rate() -> u32 {
    verballi::OUTPUT_SAMPLE_RATE
}
