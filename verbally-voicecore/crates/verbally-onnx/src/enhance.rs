//! Generic ONNX time-domain speech enhancer (e.g. a DTLN / DeepFilterNet export).
//!
//! Implements `verbally`'s `Enhancer`. The model is assumed stateless per call
//! with input `[1, frame_size]` and output `[1, frame_size]` float; adjust the
//! frame size to your export. Stateful models (with hidden-state I/O) can be
//! wired the same way by threading the extra tensors through `run`.

use ort::session::Session;
use ort::value::Tensor;

use verbally::dsp::enhance::Enhancer;

pub struct OnnxEnhancer {
    session: Session,
    frame: usize,
    in_buf: Vec<f32>,
}

impl OnnxEnhancer {
    pub fn from_file(path: &str, frame_size: usize) -> ort::Result<Self> {
        Ok(Self {
            session: Session::builder()?.commit_from_file(path)?,
            frame: frame_size,
            in_buf: Vec::new(),
        })
    }

    fn run_frame(&mut self, frame: &[f32]) -> Vec<f32> {
        let shape = vec![1i64, frame.len() as i64];
        (|| -> ort::Result<Vec<f32>> {
            let input = Tensor::from_array((shape, frame.to_vec()))?;
            let outputs = self.session.run(ort::inputs![input])?;
            let (_s, data) = outputs[0].try_extract_tensor::<f32>()?;
            Ok(data.to_vec())
        })()
        .unwrap_or_else(|_| frame.to_vec())
    }
}

impl Enhancer for OnnxEnhancer {
    fn process(&mut self, input: &[f32]) -> Vec<f32> {
        self.in_buf.extend_from_slice(input);
        let mut out = Vec::with_capacity(self.in_buf.len());
        while self.in_buf.len() >= self.frame {
            let frame: Vec<f32> = self.in_buf[..self.frame].to_vec();
            out.extend_from_slice(&self.run_frame(&frame));
            self.in_buf.drain(..self.frame);
        }
        out
    }
}
