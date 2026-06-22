//! Live path (feature = "live"): capture the mic, run it through the isolation
//! engine, and stream cleaned 16 kHz PCM into an ElevenLabs Conversational AI
//! agent over WebSocket — playing the agent's replies back out the speakers.
//!
//! This uses ElevenLabs' *published* protocol only:
//!   * connect to `wss://api.elevenlabs.io/v1/convai/conversation?agent_id=<id>`
//!   * for private agents, send the `xi-api-key` header (env `ELEVENLABS_API_KEY`)
//!   * send mic audio as `{"user_audio_chunk":"<base64 pcm16>"}`
//!   * receive `audio` events and play the base64 pcm16 back.
//!
//! No ElevenLabs SDK code is bundled; we speak the wire format directly, so this
//! stays clean-room and version-independent.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use voicecore::speaker::verify::{MfccVerifier, PresenceScorer};
use voicecore::{Config, SpeakerProfile, VoiceEngine};

use crate::Opts;

pub fn run(o: Opts) -> Result<(), String> {
    let agent_id = o.agent_id.clone().ok_or("live mode requires --agent-id <id>")?;

    // --- audio input device ------------------------------------------------
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("no default input device")?;
    let in_cfg = device
        .default_input_config()
        .map_err(|e| format!("input config: {e}"))?;
    let input_rate = in_cfg.sample_rate().0;
    eprintln!(
        "mic: {} @ {} Hz ({} ch, {:?})",
        device.name().unwrap_or_default(),
        input_rate,
        in_cfg.channels(),
        in_cfg.sample_format()
    );

    // --- engine ------------------------------------------------------------
    let mut cfg = Config::default();
    cfg.input_sample_rate = input_rate;
    cfg.denoise_strength = o.denoise.clamp(0.0, 1.0);
    cfg.speaker_focus = o.focus.clamp(0.0, 1.0);
    cfg.proximity_focus = o.proximity.clamp(0.0, 1.0);
    let mut engine = VoiceEngine::new(cfg).map_err(|e| e.to_string())?;
    let mut presence_scorer: Option<PresenceScorer> = None;
    if let Some(path) = &o.profile {
        let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
        let p = SpeakerProfile::from_bytes(&bytes).ok_or("invalid profile file")?;
        engine.set_profile(&p);
        // Reinforce the fast per-frame gate with a window-level presence score.
        // The profile centroid lives in the MfccVerifier's space, so it's a
        // direct reference. (Swap MfccVerifier for an ECAPA ONNX verifier here
        // for maximum robustness — see docs/NEURAL_MODELS.md.)
        let mut scorer = PresenceScorer::new(Box::new(MfccVerifier::new()), 1.0, 0.25);
        scorer.set_reference_embedding(p.centroid());
        presence_scorer = Some(scorer);
        eprintln!("loaded speaker profile from {path}; presence re-scoring enabled");
    } else {
        eprintln!("warning: no --profile; running denoise + proximity only (no speaker lock)");
    }

    // --- mic capture → channel of mono f32 --------------------------------
    let channels = in_cfg.channels() as usize;
    let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = mpsc::channel();
    let stream = build_input_stream(&device, &in_cfg, channels, tx)?;
    stream.play().map_err(|e| format!("start mic: {e}"))?;

    // --- playback buffer for agent audio ----------------------------------
    let playback: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
    let out_stream = build_output_stream(&host, playback.clone());

    // --- websocket to ElevenLabs ------------------------------------------
    let url = format!(
        "wss://api.elevenlabs.io/v1/convai/conversation?agent_id={}",
        agent_id
    );
    let mut req = url
        .parse::<url::Url>()
        .map_err(|e| format!("bad url: {e}"))?
        .as_str()
        .into_client_request()
        .map_err(|e| format!("request: {e}"))?;
    if let Ok(key) = std::env::var("ELEVENLABS_API_KEY") {
        req.headers_mut()
            .insert("xi-api-key", key.parse().map_err(|_| "bad api key")?);
    }
    let (mut socket, _resp) =
        tungstenite::connect(req).map_err(|e| format!("connect: {e}"))?;
    eprintln!("connected to ElevenLabs agent {agent_id}; speak now (Ctrl-C to stop)");

    // Reader thread: agent events → playback buffer.
    // tungstenite sockets aren't easily split, so we poll non-blocking in the
    // main loop instead; keep playback alive here.
    let _out_stream = out_stream; // keep it playing

    // --- main loop: pull mic, isolate, send; drain agent audio ------------
    loop {
        // Send any captured mic audio (isolated).
        while let Ok(chunk) = rx.try_recv() {
            let clean = engine.process(&chunk);
            if clean.is_empty() {
                continue;
            }
            // Window-level presence re-scoring reinforces the per-frame gate.
            if let Some(scorer) = presence_scorer.as_mut() {
                if let Some(score) = scorer.push(&clean) {
                    engine.set_speaker_presence(score);
                }
            }
            let pcm = f32_to_pcm16_le(&clean);
            let b64 = base64_encode(&pcm);
            let msg = format!("{{\"user_audio_chunk\":\"{b64}\"}}");
            if socket.send(tungstenite::Message::Text(msg)).is_err() {
                eprintln!("websocket closed; exiting");
                return Ok(());
            }
        }

        // Drain agent messages (non-blocking-ish).
        match socket.read() {
            Ok(tungstenite::Message::Text(txt)) => handle_agent_event(&txt, &playback),
            Ok(tungstenite::Message::Close(_)) => {
                eprintln!("agent closed the conversation");
                return Ok(());
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(format!("websocket error: {e}")),
        }
    }
}

fn build_input_stream(
    device: &cpal::Device,
    cfg: &cpal::SupportedStreamConfig,
    channels: usize,
    tx: Sender<Vec<f32>>,
) -> Result<cpal::Stream, String> {
    let err = |e| eprintln!("input stream error: {e}");
    let config: cpal::StreamConfig = cfg.config();
    let stream = match cfg.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _| {
                let _ = tx.send(downmix(data, channels));
            },
            err,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _| {
                let f: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                let _ = tx.send(downmix(&f, channels));
            },
            err,
            None,
        ),
        other => return Err(format!("unsupported input sample format: {other:?}")),
    };
    stream.map_err(|e| format!("build input stream: {e}"))
}

fn build_output_stream(
    host: &cpal::Host,
    playback: Arc<Mutex<VecDeque<f32>>>,
) -> Option<cpal::Stream> {
    let device = host.default_output_device()?;
    let cfg = device.default_output_config().ok()?;
    let channels = cfg.channels() as usize;
    let config: cpal::StreamConfig = cfg.config();
    // We feed 16 kHz mono; for a simple reference player we just replicate
    // samples across channels and tolerate the device rate (good enough to hear
    // the agent; a production app would resample 16k→device rate).
    let stream = device
        .build_output_stream(
            &config,
            move |out: &mut [f32], _| {
                let mut buf = playback.lock().unwrap();
                for frame in out.chunks_mut(channels) {
                    let s = buf.pop_front().unwrap_or(0.0);
                    for ch in frame.iter_mut() {
                        *ch = s;
                    }
                }
            },
            |e| eprintln!("output stream error: {e}"),
            None,
        )
        .ok()?;
    stream.play().ok()?;
    Some(stream)
}

fn handle_agent_event(txt: &str, playback: &Arc<Mutex<VecDeque<f32>>>) {
    // Minimal field extraction without a JSON dependency: look for an
    // "audio_base_64" / "audio" payload and decode it.
    for key in ["\"audio_base_64\":\"", "\"audio\":\""] {
        if let Some(start) = txt.find(key) {
            let rest = &txt[start + key.len()..];
            if let Some(end) = rest.find('"') {
                let b64 = &rest[..end];
                if let Some(pcm) = base64_decode(b64) {
                    let samples = pcm16_le_to_f32(&pcm);
                    let mut buf = playback.lock().unwrap();
                    buf.extend(samples);
                }
                return;
            }
        }
    }
}

fn downmix(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks(channels)
        .map(|f| f.iter().sum::<f32>() / channels as f32)
        .collect()
}

fn f32_to_pcm16_le(samples: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

fn pcm16_le_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / i16::MAX as f32)
        .collect()
}

// --- tiny, dependency-free base64 (standard alphabet) ----------------------

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(B64[((n >> 18) & 63) as usize] as char);
        out.push(B64[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 { B64[((n >> 6) & 63) as usize] as char } else { '=' });
        out.push(if chunk.len() > 2 { B64[(n & 63) as usize] as char } else { '=' });
    }
    out
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = s.bytes().filter(|&c| c != b'=' && !c.is_ascii_whitespace()).collect();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        let mut bits = 0;
        for &c in chunk {
            n = (n << 6) | val(c)?;
            bits += 6;
        }
        // Left-align and emit the whole bytes we have.
        n <<= 24 - bits;
        let nbytes = bits / 8;
        for i in 0..nbytes {
            out.push((n >> (16 - i * 8)) as u8);
        }
    }
    Some(out)
}

// `into_client_request` lives on this trait.
use tungstenite::client::IntoClientRequest;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips() {
        for data in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            let enc = base64_encode(data);
            let dec = base64_decode(&enc).unwrap();
            assert_eq!(dec, data, "failed for {data:?} → {enc}");
        }
    }

    #[test]
    fn pcm_round_trips() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        let pcm = f32_to_pcm16_le(&samples);
        let back = pcm16_le_to_f32(&pcm);
        for (a, b) in samples.iter().zip(back.iter()) {
            assert!((a - b).abs() < 1e-3, "{a} vs {b}");
        }
    }
}
