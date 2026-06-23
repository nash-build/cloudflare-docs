//! Live path (feature = "live"): capture the mic, run it through the isolation
//! engine, and stream cleaned 16 kHz PCM into an ElevenLabs Conversational AI
//! agent over WebSocket — playing the agent's replies back out the speakers.
//!
//! Implements ElevenLabs' published Conversational AI WebSocket protocol:
//!   * connect to `wss://api.elevenlabs.io/v1/convai/conversation?agent_id=<id>`
//!     (public agents) or a **signed URL** for private agents (needs an API key),
//!   * on open, send `{"type":"conversation_initiation_client_data"}`,
//!   * stream mic audio as `{"user_audio_chunk":"<base64 pcm16>"}`,
//!   * reply to `ping` events with `pong` (keepalive — required),
//!   * play `audio` events and clear playback on `interruption`.
//!
//! No ElevenLabs SDK code is bundled; we speak the wire format directly.

use std::collections::VecDeque;
use std::net::TcpStream;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use serde_json::{json, Value};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::Message;

use voicecore::speaker::verify::{MfccVerifier, PresenceScorer};
use voicecore::{Config, SpeakerProfile, VoiceEngine};

use crate::Opts;

/// ElevenLabs agents emit PCM at this rate by default (`pcm_16000`). If your
/// agent's output format differs, change this.
const AGENT_OUTPUT_RATE: u32 = 16_000;

pub fn run(o: Opts) -> Result<(), String> {
    let agent_id = o.agent_id.clone().ok_or("live mode requires --agent-id <id>")?;

    // --- audio input device ------------------------------------------------
    let host = cpal::default_host();
    let device = host.default_input_device().ok_or("no default input device")?;
    let in_cfg = device.default_input_config().map_err(|e| format!("input config: {e}"))?;
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
        let mut scorer = PresenceScorer::new(Box::new(MfccVerifier::new()), 1.0, 0.25);
        scorer.set_reference_embedding(p.centroid());
        presence_scorer = Some(scorer);
        eprintln!("loaded speaker profile from {path}; presence re-scoring enabled");
    } else {
        eprintln!("warning: no --profile; running denoise + proximity only (no speaker lock)");
    }
    if o.deepfilter > 0.0 {
        engine
            .pipeline_mut()
            .set_enhancer(Box::new(voicecore_deepfilter::ErbEnhancer::new_dsp(o.deepfilter.clamp(0.0, 1.0))));
        eprintln!("DeepFilterNet ERB enhancer enabled (strength {})", o.deepfilter);
    }

    // --- mic capture → channel of mono f32 --------------------------------
    let channels = in_cfg.channels() as usize;
    let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = mpsc::channel();
    let stream = build_input_stream(&device, &in_cfg, channels, tx)?;
    stream.play().map_err(|e| format!("start mic: {e}"))?;

    // --- playback buffer for agent audio (stored at the device output rate) -
    let playback: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
    let (out_stream, out_rate) = build_output_stream(&host, playback.clone());
    let _out_stream = out_stream; // keep it alive/playing

    // --- resolve the WebSocket URL (signed for private agents) ------------
    let url = match std::env::var("ELEVENLABS_API_KEY") {
        Ok(key) if !key.is_empty() => {
            eprintln!("requesting signed URL for private agent…");
            signed_url(&agent_id, &key)?
        }
        _ => format!("wss://api.elevenlabs.io/v1/convai/conversation?agent_id={agent_id}"),
    };

    let (mut socket, _resp) = tungstenite::connect(url.as_str()).map_err(|e| format!("connect: {e}"))?;
    // Poll reads with a short timeout so the loop can also pump mic audio.
    set_read_timeout(socket.get_ref(), Duration::from_millis(15));
    // Begin the conversation.
    let _ = socket.send(Message::Text(json!({"type":"conversation_initiation_client_data"}).to_string().into()));
    eprintln!("connected to ElevenLabs agent {agent_id}; speak now (Ctrl-C to stop)");

    // --- main loop: pump mic out, drain agent events in -------------------
    loop {
        // Send any captured mic audio (isolated).
        while let Ok(chunk) = rx.try_recv() {
            let clean = engine.process(&chunk);
            if clean.is_empty() {
                continue;
            }
            if let Some(scorer) = presence_scorer.as_mut() {
                if let Some(score) = scorer.push(&clean) {
                    engine.set_speaker_presence(score);
                }
            }
            let b64 = base64_encode(&f32_to_pcm16_le(&clean));
            if socket
                .send(Message::Text(json!({ "user_audio_chunk": b64 }).to_string().into()))
                .is_err()
            {
                eprintln!("websocket closed; exiting");
                return Ok(());
            }
        }

        // Drain one agent message (read times out ~15 ms so we loop back to mic).
        match socket.read() {
            Ok(Message::Text(t)) => handle_event(t.as_str(), &mut socket, &playback, out_rate),
            Ok(Message::Ping(p)) => {
                let _ = socket.send(Message::Pong(p));
            }
            Ok(Message::Close(_)) => {
                eprintln!("agent closed the conversation");
                return Ok(());
            }
            Ok(_) => {}
            Err(tungstenite::Error::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(tungstenite::Error::ConnectionClosed) | Err(tungstenite::Error::AlreadyClosed) => {
                return Ok(())
            }
            Err(e) => return Err(format!("websocket error: {e}")),
        }
    }
}

/// Handle one decoded ElevenLabs JSON event.
fn handle_event(
    text: &str,
    socket: &mut tungstenite::WebSocket<MaybeTlsStream<TcpStream>>,
    playback: &Arc<Mutex<VecDeque<f32>>>,
    out_rate: u32,
) {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return };
    match v["type"].as_str() {
        Some("ping") => {
            // Keepalive: echo the event_id back as a pong.
            let id = v["ping_event"]["event_id"].clone();
            let _ = socket.send(Message::Text(json!({"type":"pong","event_id": id}).to_string().into()));
        }
        Some("audio") => {
            if let Some(b64) = v["audio_event"]["audio_base_64"].as_str() {
                if let Some(pcm) = base64_decode(b64) {
                    let samples = pcm16_le_to_f32(&pcm);
                    let resampled = resample_linear(&samples, AGENT_OUTPUT_RATE, out_rate);
                    playback.lock().unwrap().extend(resampled);
                }
            }
        }
        Some("interruption") => {
            // The user barged in — stop playing queued agent audio.
            playback.lock().unwrap().clear();
        }
        Some("agent_response") => {
            if let Some(t) = v["agent_response_event"]["agent_response"].as_str() {
                eprintln!("agent: {t}");
            }
        }
        _ => {}
    }
}

/// Fetch a signed WebSocket URL for a private agent.
fn signed_url(agent_id: &str, api_key: &str) -> Result<String, String> {
    let url = format!(
        "https://api.elevenlabs.io/v1/convai/conversation/get-signed-url?agent_id={agent_id}"
    );
    let body = ureq::get(&url)
        .set("xi-api-key", api_key)
        .call()
        .map_err(|e| format!("signed-url request: {e}"))?
        .into_string()
        .map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    v["signed_url"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| format!("no signed_url in response: {body}"))
}

fn set_read_timeout(stream: &MaybeTlsStream<TcpStream>, d: Duration) {
    let _ = match stream {
        MaybeTlsStream::Plain(s) => s.set_read_timeout(Some(d)),
        MaybeTlsStream::Rustls(s) => s.sock.set_read_timeout(Some(d)),
        _ => Ok(()),
    };
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

/// Returns the output stream and its sample rate (so the agent's 16 kHz audio
/// can be resampled to it before playback).
fn build_output_stream(
    host: &cpal::Host,
    playback: Arc<Mutex<VecDeque<f32>>>,
) -> (Option<cpal::Stream>, u32) {
    let Some(device) = host.default_output_device() else { return (None, 48_000) };
    let Ok(cfg) = device.default_output_config() else { return (None, 48_000) };
    let out_rate = cfg.sample_rate().0;
    let channels = cfg.channels() as usize;
    let config: cpal::StreamConfig = cfg.config();
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
        .ok();
    if let Some(s) = &stream {
        let _ = s.play();
    }
    (stream, out_rate)
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

fn resample_linear(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let ratio = to as f32 / from as f32;
    let out_len = (input.len() as f32 * ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f32 / ratio;
        let i0 = src.floor() as usize;
        let frac = src - i0 as f32;
        let a = input.get(i0).copied().unwrap_or(0.0);
        let b = input.get(i0 + 1).copied().unwrap_or(a);
        out.push(a + (b - a) * frac);
    }
    out
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
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
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
        n <<= 24 - bits;
        let nbytes = bits / 8;
        for i in 0..nbytes {
            out.push((n >> (16 - i * 8)) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_round_trips() {
        for data in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar"] {
            let enc = base64_encode(data);
            assert_eq!(base64_decode(&enc).unwrap(), data);
        }
    }

    #[test]
    fn pcm_round_trips() {
        let samples = vec![0.0f32, 0.5, -0.5, 1.0, -1.0];
        let back = pcm16_le_to_f32(&f32_to_pcm16_le(&samples));
        for (a, b) in samples.iter().zip(back.iter()) {
            assert!((a - b).abs() < 1e-3);
        }
    }

    #[test]
    fn resample_changes_length_by_ratio() {
        let input = vec![0.0f32; 1600]; // 0.1 s @ 16k
        let out = resample_linear(&input, 16_000, 48_000);
        assert!((out.len() as i32 - 4800).abs() <= 2);
    }
}
