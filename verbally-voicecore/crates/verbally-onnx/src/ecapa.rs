//! ECAPA-TDNN speaker verifier backed by ONNX Runtime.
//!
//! Implements `verbally`'s `SpeakerVerifier`, so it drops straight into a
//! `PresenceScorer` for a trained-model identity lock. Feeds the model log-mel
//! features from [`crate::features`].
//!
//! Input/output assumptions (adjust to your export):
//! * input  : `[1, n_frames, n_mels]` float log-mel features (single input),
//! * output : `[1, embedding_dim]` float speaker embedding.
//!
//! Runtime: ONNX Runtime is dynamically loaded — set `ORT_DYLIB_PATH` to your
//! `libonnxruntime` (or ship it beside the binary). On Apple/Android, build a
//! variant with the CoreML/NNAPI execution providers for acceleration.

use ort::session::Session;
use ort::value::Tensor;

use verbally::speaker::verify::SpeakerVerifier;

use crate::features::{MelConfig, MelFrontend};
use crate::l2_normalise;

pub struct OnnxEcapaVerifier {
    session: Session,
    frontend: MelFrontend,
    dim: usize,
}

impl OnnxEcapaVerifier {
    /// Load with the default 80-mel front-end and a 192-d embedding.
    pub fn from_file(path: &str) -> ort::Result<Self> {
        Self::with_config(path, MelConfig::default(), 192)
    }

    /// Load with explicit mel parameters and embedding dimensionality.
    pub fn with_config(path: &str, mel: MelConfig, embedding_dim: usize) -> ort::Result<Self> {
        let session = Session::builder()?.commit_from_file(path)?;
        Ok(Self { session, frontend: MelFrontend::new(mel), dim: embedding_dim })
    }
}

impl SpeakerVerifier for OnnxEcapaVerifier {
    fn dim(&self) -> usize {
        self.dim
    }

    fn embed_utterance(&mut self, audio_16k: &[f32]) -> Vec<f32> {
        let (feats, n) = self.frontend.features(audio_16k);
        let n_mels = self.frontend.n_mels();
        if n == 0 {
            return vec![0.0; self.dim];
        }
        let shape = vec![1i64, n as i64, n_mels as i64];
        let mut emb = (|| -> ort::Result<Vec<f32>> {
            let input = Tensor::from_array((shape, feats))?;
            let outputs = self.session.run(ort::inputs![input])?;
            let (_s, data) = outputs[0].try_extract_tensor::<f32>()?;
            Ok(data.to_vec())
        })()
        .unwrap_or_else(|_| vec![0.0; self.dim]);
        l2_normalise(&mut emb);
        emb
    }
}
