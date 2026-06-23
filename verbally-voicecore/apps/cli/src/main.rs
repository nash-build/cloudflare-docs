//! `verbally` reference CLI.
//!
//! Subcommands:
//!   enroll  <in.wav> <profile.bin>                 build a voice fingerprint
//!   process <in.wav> <out.wav> [opts]              isolate a file to 16 kHz mono
//!   bench   <in.wav>                               measure real-time factor
//!   live    --agent-id <id> [opts]   (feature=live) clean mic → ElevenLabs agent
//!
//! process/live options:
//!   --profile <file>     speaker profile to keep (from `enroll`)
//!   --denoise <0..1>     suppression strength (default 0.75)
//!   --focus <0..1>       speaker focus (default 0.8)
//!   --proximity <0..1>   near-field focus (default 0.4)

use std::env;
use std::process::ExitCode;

use verbally::{Config, SpeakerProfile, VoiceEngine};

mod wav;
#[cfg(feature = "live")]
mod live;

struct Opts {
    profile: Option<String>,
    denoise: f32,
    focus: f32,
    proximity: f32,
    deepfilter: f32,
    agent_id: Option<String>,
    input_rate: u32,
}

impl Default for Opts {
    fn default() -> Self {
        let d = Config::default();
        Self {
            profile: None,
            denoise: d.denoise_strength,
            focus: d.speaker_focus,
            proximity: d.proximity_focus,
            deepfilter: 0.0,
            agent_id: None,
            input_rate: 48_000,
        }
    }
}

fn parse_opts(args: &[String]) -> Opts {
    let mut o = Opts::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--profile" => { o.profile = args.get(i + 1).cloned(); i += 2; }
            "--denoise" => { o.denoise = num(args.get(i + 1)); i += 2; }
            "--focus" => { o.focus = num(args.get(i + 1)); i += 2; }
            "--proximity" => { o.proximity = num(args.get(i + 1)); i += 2; }
            "--deepfilter" => { o.deepfilter = num(args.get(i + 1)); i += 2; }
            "--agent-id" => { o.agent_id = args.get(i + 1).cloned(); i += 2; }
            "--input-rate" => { o.input_rate = num(args.get(i + 1)) as u32; i += 2; }
            _ => { i += 1; }
        }
    }
    o
}

fn num(s: Option<&String>) -> f32 {
    s.and_then(|x| x.parse().ok()).unwrap_or(0.0)
}

fn build_engine(rate: u32, o: &Opts) -> Result<VoiceEngine, String> {
    let mut cfg = Config::default();
    cfg.input_sample_rate = rate;
    cfg.denoise_strength = o.denoise.clamp(0.0, 1.0);
    cfg.speaker_focus = o.focus.clamp(0.0, 1.0);
    cfg.proximity_focus = o.proximity.clamp(0.0, 1.0);
    let mut engine = VoiceEngine::new(cfg).map_err(|e| e.to_string())?;
    if let Some(path) = &o.profile {
        let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
        let p = SpeakerProfile::from_bytes(&bytes).ok_or("invalid profile file")?;
        engine.set_profile(&p);
        eprintln!("loaded speaker profile from {path}");
    }
    if o.deepfilter > 0.0 {
        use verbally_deepfilter::ErbEnhancer;
        engine
            .pipeline_mut()
            .set_enhancer(Box::new(ErbEnhancer::new_dsp(o.deepfilter.clamp(0.0, 1.0))));
        eprintln!("DeepFilterNet ERB enhancer enabled (strength {})", o.deepfilter);
    }
    Ok(engine)
}

fn cmd_enroll(args: &[String]) -> Result<(), String> {
    let input = args.first().ok_or("usage: enroll <in.wav> <profile.bin>")?;
    let out = args.get(1).ok_or("usage: enroll <in.wav> <profile.bin>")?;
    let (samples, rate) = wav::read_mono_f32(input)?;
    let secs = samples.len() as f32 / rate as f32;
    if secs < 5.0 {
        eprintln!("warning: only {secs:.1}s of audio; 20–30s of clean speech is recommended");
    }
    let mut cfg = Config::default();
    cfg.input_sample_rate = rate;
    let mut engine = VoiceEngine::new(cfg).map_err(|e| e.to_string())?;
    engine.begin_enrollment();
    let _ = engine.process(&samples);
    let profile = engine.finish_enrollment().ok_or("enrollment produced no profile")?;
    std::fs::write(out, profile.to_bytes()).map_err(|e| format!("write {out}: {e}"))?;
    println!("enrolled {:.1}s ({} frames) → {out}", secs, profile.frames());
    Ok(())
}

fn cmd_process(args: &[String]) -> Result<(), String> {
    let input = args.first().ok_or("usage: process <in.wav> <out.wav> [opts]")?;
    let output = args.get(1).ok_or("usage: process <in.wav> <out.wav> [opts]")?;
    let o = parse_opts(&args[2.min(args.len())..]);
    let (samples, rate) = wav::read_mono_f32(input)?;
    let mut engine = build_engine(rate, &o)?;
    let clean = engine.process(&samples);
    wav::write_mono_f32(output, &clean, verbally::OUTPUT_SAMPLE_RATE)?;
    println!(
        "processed {} samples @ {}Hz → {} samples @ {}Hz ({output})",
        samples.len(),
        rate,
        clean.len(),
        verbally::OUTPUT_SAMPLE_RATE
    );
    Ok(())
}

fn cmd_bench(args: &[String]) -> Result<(), String> {
    let input = args.first().ok_or("usage: bench <in.wav>")?;
    let (samples, rate) = wav::read_mono_f32(input)?;
    let audio_secs = samples.len() as f64 / rate as f64;
    let mut engine = build_engine(rate, &Opts::default())?;
    // Feed in 20 ms chunks to mimic a live capture cadence.
    let chunk = (rate as usize / 50).max(1);
    let start = std::time::Instant::now();
    let mut produced = 0usize;
    for c in samples.chunks(chunk) {
        produced += engine.process(c).len();
    }
    let elapsed = start.elapsed().as_secs_f64();
    let rtf = elapsed / audio_secs;
    println!("audio: {audio_secs:.2}s  compute: {elapsed:.3}s  real-time factor: {rtf:.3}x");
    println!("produced {produced} output samples  (RTF < 1.0 means faster than real time)");
    Ok(())
}

fn usage() -> ExitCode {
    eprintln!(
        "verbally {}\n\nUSAGE:\n  verbally enroll  <in.wav> <profile.bin>\n  verbally process <in.wav> <out.wav> [--profile f] [--denoise x] [--focus x] [--proximity x] [--deepfilter x]\n  verbally bench   <in.wav>\n  verbally live    --agent-id <id> [--profile f] [--input-rate 48000]   (build with --features live)",
        verbally::VERSION
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(cmd) = args.first() else { return usage() };
    let rest = &args[1..];
    let result = match cmd.as_str() {
        "enroll" => cmd_enroll(rest),
        "process" => cmd_process(rest),
        "bench" => cmd_bench(rest),
        "live" => run_live(rest),
        "-h" | "--help" | "help" => return usage(),
        other => Err(format!("unknown command: {other}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(feature = "live")]
fn run_live(args: &[String]) -> Result<(), String> {
    let o = parse_opts(args);
    live::run(o)
}

#[cfg(not(feature = "live"))]
fn run_live(_args: &[String]) -> Result<(), String> {
    Err("this build has no live support; rebuild with `--features live`".into())
}
