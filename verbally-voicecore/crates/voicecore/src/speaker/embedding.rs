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

/// Number of mel filterbank channels for the MFCC embedder.
pub const MELS: usize = 40;
/// Number of cepstral coefficients kept (C1..=MFCC; C0/energy dropped).
pub const MFCC: usize = 20;

/// MFCC-based speaker embedder.
///
/// A mel filterbank → log → DCT-II produces a compact cepstral descriptor — the
/// standard, far stronger voice fingerprint than raw band energies, and the same
/// feature family ECAPA-TDNN/x-vector front-ends consume. It is
/// cepstral-mean-subtracted and L2-normalised so it keys on vocal-tract *timbre*
/// rather than loudness or channel colour. This is the default embedder.
///
/// For maximum separation between very similar voices, swap in an ECAPA-TDNN
/// ONNX model behind the `neural` feature (see `speaker::neural`); it implements
/// the same [`Embedder`] trait and feeds the same gate unchanged.
pub struct MfccEmbedder {
    /// Sparse triangular filterbank: per mel band, (first_bin, weights).
    filters: Vec<(usize, Vec<f32>)>,
    /// DCT-II basis, `MFCC` rows of `MELS` cosines.
    dct: Vec<[f32; MELS]>,
}

impl MfccEmbedder {
    pub fn new(n_bins: usize, sample_rate: u32) -> Self {
        let fmax = sample_rate as f32 / 2.0;
        let (mel_min, mel_max) = (hz_to_mel(0.0), hz_to_mel(fmax));

        // MELS+2 band edges, equally spaced on the mel scale.
        let edges_hz: Vec<f32> = (0..MELS + 2)
            .map(|i| {
                let mel = mel_min + (mel_max - mel_min) * i as f32 / (MELS + 1) as f32;
                mel_to_hz(mel)
            })
            .collect();
        let bin_of = |hz: f32| (n_bins as f32 - 1.0) * hz / fmax;

        let mut filters = Vec::with_capacity(MELS);
        for m in 1..=MELS {
            let (left, center, right) =
                (bin_of(edges_hz[m - 1]), bin_of(edges_hz[m]), bin_of(edges_hz[m + 1]));
            let start = left.floor().max(0.0) as usize;
            let end = (right.ceil() as usize).min(n_bins.saturating_sub(1));
            let mut weights = Vec::with_capacity(end.saturating_sub(start) + 1);
            for k in start..=end {
                let kf = k as f32;
                let w = if kf <= center {
                    if center > left { (kf - left) / (center - left) } else { 0.0 }
                } else if right > center {
                    (right - kf) / (right - center)
                } else {
                    0.0
                };
                weights.push(w.clamp(0.0, 1.0));
            }
            filters.push((start, weights));
        }

        let mut dct = Vec::with_capacity(MFCC);
        for i in 1..=MFCC {
            let mut row = [0.0f32; MELS];
            for (b, r) in row.iter_mut().enumerate() {
                *r = (std::f32::consts::PI * i as f32 * (b as f32 + 0.5) / MELS as f32).cos();
            }
            dct.push(row);
        }
        Self { filters, dct }
    }
}

impl Embedder for MfccEmbedder {
    fn dim(&self) -> usize {
        MFCC
    }

    fn embed(&self, magnitude: &[f32]) -> Vec<f32> {
        // Log mel-band energies.
        let mut logmel = [0.0f32; MELS];
        for (m, (start, weights)) in self.filters.iter().enumerate() {
            let mut acc = 0.0f32;
            for (j, &w) in weights.iter().enumerate() {
                let k = start + j;
                if k < magnitude.len() {
                    acc += w * magnitude[k] * magnitude[k];
                }
            }
            logmel[m] = (acc + 1e-9).ln();
        }
        // DCT-II → cepstral coefficients (C1..=MFCC).
        let mut v = vec![0.0f32; MFCC];
        for (i, row) in self.dct.iter().enumerate() {
            v[i] = logmel.iter().zip(row.iter()).map(|(&l, &c)| l * c).sum();
        }
        // Cepstral mean subtraction + unit norm.
        let mean = v.iter().sum::<f32>() / MFCC as f32;
        for x in &mut v {
            *x -= mean;
        }
        l2_normalise(&mut v);
        v
    }
}

#[inline]
fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

#[inline]
fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10f32.powf(mel / 2595.0) - 1.0)
}
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

    // A more voice-like spectrum: a pitch with formant peaks, plus noise floor.
    fn formant_voice(f0_bin: usize, formants: &[usize], n: usize) -> Vec<f32> {
        let mut m = vec![0.03f32; n];
        // harmonics of f0
        let mut h = f0_bin;
        while h < n {
            m[h] += 0.5;
            h += f0_bin;
        }
        // formant resonances
        for &f in formants {
            for d in 0..6 {
                if f + d < n {
                    m[f + d] += 0.8 * (1.0 - d as f32 / 6.0);
                }
            }
        }
        m
    }

    #[test]
    fn mfcc_is_unit_norm_and_right_dim() {
        let emb = MfccEmbedder::new(257, 16_000);
        let v = emb.embed(&voice_low(257));
        assert_eq!(v.len(), MFCC);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn mfcc_separates_two_voices_strongly() {
        let n = 257;
        let emb = MfccEmbedder::new(n, 16_000);
        // "me": low pitch, formants around bins 12 and 40.
        let me_spec = |()| formant_voice(8, &[12, 40], n);
        // "other": higher pitch, formants around bins 30 and 90.
        let other_spec = |()| formant_voice(14, &[30, 90], n);

        let mut me = SpeakerProfile::new(emb.dim());
        for _ in 0..30 {
            me.add(&emb.embed(&me_spec(())));
        }
        let c = me.centroid();
        let self_sim = cosine(&c, &emb.embed(&me_spec(())));
        let other_sim = cosine(&c, &emb.embed(&other_spec(())));
        // MFCC should give a clear margin between the two voices.
        assert!(self_sim > 0.8, "self too low: {self_sim}");
        assert!(self_sim - other_sim > 0.3, "weak margin: self {self_sim} other {other_sim}");
    }
}
