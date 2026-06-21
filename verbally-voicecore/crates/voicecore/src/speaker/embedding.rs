//! Speaker fingerprinting.
//!
//! The built-in [`BandEmbedder`] turns a magnitude spectrum into a compact,
//! L2-normalised timbre vector (log band-energies with mean removal — a
//! cepstral-flavoured descriptor). Averaged over an enrollment clip it forms a
//! stable centroid for *your* voice that the gate compares against frame by
//! frame.
//!
//! This is deliberately lightweight and dependency-free so the whole engine
//! builds and runs anywhere today. For maximum separation between similar
//! voices, implement [`Embedder`] around an ECAPA-TDNN ONNX model (run via the
//! `ort` crate with CoreML/NNAPI acceleration) and feed its 192-d embedding to
//! the same [`SpeakerProfile`]/gate — nothing else changes.

/// Produces a per-frame speaker embedding from a magnitude spectrum.
pub trait Embedder: Send {
    /// Embedding dimensionality.
    fn dim(&self) -> usize;
    /// Embed one magnitude frame (first FFT_SIZE/2+1 bins). Returns an
    /// L2-normalised vector of length [`dim`](Embedder::dim).
    fn embed(&self, magnitude: &[f32]) -> Vec<f32>;
}

/// Number of log-spaced bands in the built-in descriptor.
pub const BANDS: usize = 24;

/// Built-in band-energy embedder.
pub struct BandEmbedder {
    // Precomputed [start,end) bin index per band.
    bands: Vec<(usize, usize)>,
}

impl BandEmbedder {
    /// Build a log-spaced band layout over `n_bins` (= FFT_SIZE/2+1).
    pub fn new(n_bins: usize) -> Self {
        let mut bands = Vec::with_capacity(BANDS);
        // Log spacing from bin 1 (skip DC) to the top bin.
        let lo = 1.0f32.ln();
        let hi = (n_bins as f32 - 1.0).max(2.0).ln();
        let mut prev = 1usize;
        for b in 1..=BANDS {
            let edge = (lo + (hi - lo) * b as f32 / BANDS as f32).exp();
            let end = (edge.round() as usize).clamp(prev + 1, n_bins);
            bands.push((prev, end));
            prev = end;
        }
        Self { bands }
    }
}

impl Embedder for BandEmbedder {
    fn dim(&self) -> usize {
        BANDS
    }

    fn embed(&self, magnitude: &[f32]) -> Vec<f32> {
        let mut v = vec![0.0f32; BANDS];
        for (i, &(s, e)) in self.bands.iter().enumerate() {
            let mut acc = 0.0f32;
            for k in s..e.min(magnitude.len()) {
                acc += magnitude[k] * magnitude[k];
            }
            let width = (e - s).max(1) as f32;
            v[i] = (acc / width + 1e-9).ln();
        }
        // Remove the mean (cepstral mean subtraction) → robust to overall level
        // and broadband gain, keeps the *shape* that identifies a voice.
        let mean = v.iter().sum::<f32>() / BANDS as f32;
        for x in &mut v {
            *x -= mean;
        }
        l2_normalise(&mut v);
        v
    }
}

/// Accumulates per-frame embeddings during enrollment into one centroid.
#[derive(Clone)]
pub struct SpeakerProfile {
    centroid: Vec<f32>,
    count: u64,
}

impl SpeakerProfile {
    pub fn new(dim: usize) -> Self {
        Self { centroid: vec![0.0; dim], count: 0 }
    }

    /// Restore a previously saved centroid (e.g. from disk).
    pub fn from_centroid(mut centroid: Vec<f32>) -> Self {
        l2_normalise(&mut centroid);
        Self { centroid, count: 1 }
    }

    /// Add one enrollment frame.
    pub fn add(&mut self, embedding: &[f32]) {
        debug_assert_eq!(embedding.len(), self.centroid.len());
        for (c, &e) in self.centroid.iter_mut().zip(embedding) {
            *c += e;
        }
        self.count += 1;
    }

    pub fn frames(&self) -> u64 {
        self.count
    }

    /// Finalise the mean direction. Cheap; call when enrollment ends.
    pub fn centroid(&self) -> Vec<f32> {
        let mut c = self.centroid.clone();
        if self.count > 0 {
            for x in &mut c {
                *x /= self.count as f32;
            }
        }
        l2_normalise(&mut c);
        c
    }
}

/// Cosine similarity of two L2-normalised vectors → just the dot product,
/// returned in `[-1, 1]`.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>().clamp(-1.0, 1.0)
}

fn l2_normalise(v: &mut [f32]) {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 1e-9 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Two distinct synthetic "voices": energy in different band regions.
    fn voice_low(n: usize) -> Vec<f32> {
        let mut m = vec![0.02f32; n];
        for k in 5..25 {
            m[k] = 1.0;
        }
        m
    }
    fn voice_high(n: usize) -> Vec<f32> {
        let mut m = vec![0.02f32; n];
        for k in 120..170 {
            m[k] = 1.0;
        }
        m
    }

    #[test]
    fn same_voice_scores_higher_than_different() {
        let n = 257;
        let emb = BandEmbedder::new(n);
        let mut me = SpeakerProfile::new(emb.dim());
        for _ in 0..20 {
            me.add(&emb.embed(&voice_low(n)));
        }
        let centroid = me.centroid();

        let self_sim = cosine(&centroid, &emb.embed(&voice_low(n)));
        let other_sim = cosine(&centroid, &emb.embed(&voice_high(n)));
        assert!(self_sim > other_sim + 0.3, "self {self_sim} other {other_sim}");
    }

    #[test]
    fn embedding_is_unit_norm() {
        let emb = BandEmbedder::new(257);
        let v = emb.embed(&voice_low(257));
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }
}
