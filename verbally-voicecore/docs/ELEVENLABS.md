# ElevenLabs integration

The engine's only job is to hand ElevenLabs **clean 16 kHz mono PCM**. The
connection itself uses ElevenLabs' own published protocol/SDK in your shell —
nothing here wraps or modifies their code.

## Where it plugs in

```
mic → voicecore engine → clean 16 kHz mono → ElevenLabs (STS / agent, your agent_id)
```

The engine downsamples, denoises, and speaker-gates; ElevenLabs receives audio
that sounds like only you in a quiet room.

## Conversational-AI agent (WebSocket)

The reference CLI (`apps/cli/src/live.rs`, `--features live`) speaks the wire
protocol directly:

- **Connect:** `wss://api.elevenlabs.io/v1/convai/conversation?agent_id=<AGENT_ID>`
- **Auth (private agents):** send the `xi-api-key` header (env
  `ELEVENLABS_API_KEY`). Public agents need no key. For production, prefer a
  short-lived **signed URL** minted by your backend over shipping the API key in
  the client.
- **Send mic audio:** `{"user_audio_chunk":"<base64 pcm16>"}` — exactly the
  engine's output, converted f32→pcm16→base64.
- **Receive + play:** decode the base64 audio in the agent's `audio` events.

```bash
export ELEVENLABS_API_KEY=sk_...     # private agents only
./target/release/voicecore live --agent-id <AGENT_ID> --profile you.profile
```

## Using the official SDKs in the app shells

You don't have to hand-roll the socket. In the native apps, use the engine for
audio and the ElevenLabs SDK for transport:

- **Web/Electron:** `@elevenlabs/client` / `@elevenlabs/react` — feed the
  AudioWorklet's cleaned output as the input track.
- **iOS / Android:** the ElevenLabs mobile SDKs — set the engine's output as the
  microphone source you stream.

This split is the clean-room boundary: **our** code makes clean audio; **their**
SDK moves it. Keeping them separate means engine upgrades and ElevenLabs SDK
upgrades never collide.

## Speech-to-speech (STS) REST/stream

For the STS endpoint (non-agent), POST/stream the engine's 16 kHz mono PCM as the
input audio with your chosen `voice_id`. Same principle: the engine sits in front,
ElevenLabs' API is called per their docs.

## Tuning for turn-taking

ElevenLabs runs its own VAD/turn detection on the audio you send. Over-suppression
can starve it and make turns feel laggy. The defaults are chosen to avoid this:

- `speaker_gate_floor = 0.05` keeps a little signal so the line never sounds dead.
- `denoise_strength = 0.75` cleans aggressively without gating word edges.

If turns feel cut off, lower `denoise_strength` or raise `speaker_gate_floor`.
