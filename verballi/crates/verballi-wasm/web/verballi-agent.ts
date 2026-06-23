// Framework-agnostic browser client: mic → verballi (WASM) isolation →
// ElevenLabs Conversational AI agent, with the agent's replies played back.
//
// Implements ElevenLabs' published WebSocket protocol (conversation init,
// ping/pong keepalive, audio playback, interruption) — the same flow validated
// in the native `live` client. The ElevenLabs JS SDK is intentionally not used
// for transport so we can insert isolated audio; clean-room split preserved.
//
// You provide a `WasmEngine` factory (from your wasm-pack build) and either a
// public `agentId` or a `getSignedUrl()` that calls YOUR backend (never put the
// ElevenLabs API key in the browser).

export interface VerballiEngine {
  process(input: Float32Array): Float32Array;
  begin_enrollment(): void;
  enrollment_frames(): number;
  finish_enrollment(): Uint8Array;
  set_profile(bytes: Uint8Array): boolean;
  is_enrolled(): boolean;
  set_speaker_presence(score: number): void;
}

export interface VerballiAgentOptions {
  /** Create an initialized engine for the given mic sample rate. */
  createEngine: (sampleRate: number) => Promise<VerballiEngine> | VerballiEngine;
  /** Public agent id (used if getSignedUrl is not provided). */
  agentId?: string;
  /** Returns a signed wss URL from your backend (for private agents). */
  getSignedUrl?: () => Promise<string>;
  /** Optional saved speaker profile to keep only the enrolled voice. */
  profile?: Uint8Array;
  /** URL of the capture worklet module (default '/capture-worklet.js'). */
  workletUrl?: string;
  /** ElevenLabs agent audio output rate (default 16000). */
  agentOutputRate?: number;
  onAgentText?: (text: string) => void;
  onStatus?: (status: "connecting" | "live" | "stopped") => void;
  onError?: (err: unknown) => void;
}

export class VerballiAgent {
  private opts: VerballiAgentOptions;
  private ctx?: AudioContext;
  private engine?: VerballiEngine;
  private ws?: WebSocket;
  private node?: AudioWorkletNode;
  private media?: MediaStream;
  private playHead = 0; // scheduled playback time
  private sources: AudioBufferSourceNode[] = [];
  private running = false;

  constructor(opts: VerballiAgentOptions) {
    this.opts = opts;
  }

  async start(): Promise<void> {
    if (this.running) return;
    this.running = true;
    this.opts.onStatus?.("connecting");

    // 1) Mic + audio graph.
    this.media = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: true, noiseSuppression: false, autoGainControl: false },
    });
    this.ctx = new AudioContext();
    const sampleRate = this.ctx.sampleRate;
    await this.ctx.audioWorklet.addModule(this.opts.workletUrl ?? "/capture-worklet.js");

    // 2) Engine.
    this.engine = await this.opts.createEngine(sampleRate);
    if (this.opts.profile) this.engine.set_profile(this.opts.profile);

    // 3) WebSocket to ElevenLabs.
    const url = this.opts.getSignedUrl
      ? await this.opts.getSignedUrl()
      : `wss://api.elevenlabs.io/v1/convai/conversation?agent_id=${this.opts.agentId}`;
    this.ws = new WebSocket(url);
    this.ws.onopen = () => {
      this.send({ type: "conversation_initiation_client_data" });
      this.opts.onStatus?.("live");
    };
    this.ws.onmessage = (e) => this.onMessage(e);
    this.ws.onerror = (e) => this.opts.onError?.(e);
    this.ws.onclose = () => this.stop();

    // 4) Mic frames → engine → ElevenLabs.
    const src = this.ctx.createMediaStreamSource(this.media);
    this.node = new AudioWorkletNode(this.ctx, "verballi-capture");
    this.node.port.onmessage = (ev: MessageEvent<Float32Array>) => {
      if (!this.engine || this.ws?.readyState !== WebSocket.OPEN) return;
      const clean = this.engine.process(ev.data);
      if (clean.length === 0) return;
      this.send({ user_audio_chunk: f32ToBase64Pcm16(clean) });
    };
    src.connect(this.node);
    // Worklet needs a sink to keep pulling; route through a muted gain.
    const sink = this.ctx.createGain();
    sink.gain.value = 0;
    this.node.connect(sink).connect(this.ctx.destination);
  }

  stop(): void {
    if (!this.running) return;
    this.running = false;
    this.stopPlayback();
    try { this.ws?.close(); } catch {}
    this.media?.getTracks().forEach((t) => t.stop());
    this.ctx?.close().catch(() => {});
    this.ws = undefined;
    this.node = undefined;
    this.opts.onStatus?.("stopped");
  }

  // --- Enrollment helpers (call these on a separate, pre-conversation pass) --

  /** Run a short enrollment over a clip of the user's voice; returns the
   *  profile bytes you should persist and pass back via `options.profile`. */
  async enroll(voice16k: Float32Array, sampleRate: number): Promise<Uint8Array> {
    const engine = await this.opts.createEngine(sampleRate);
    engine.begin_enrollment();
    engine.process(voice16k);
    return engine.finish_enrollment();
  }

  // --- internals ---------------------------------------------------------

  private send(obj: unknown) {
    if (this.ws?.readyState === WebSocket.OPEN) this.ws.send(JSON.stringify(obj));
  }

  private onMessage(e: MessageEvent) {
    let msg: any;
    try { msg = JSON.parse(e.data); } catch { return; }
    switch (msg.type) {
      case "ping":
        // Keepalive: echo event_id back as pong after the suggested delay.
        setTimeout(
          () => this.send({ type: "pong", event_id: msg.ping_event?.event_id }),
          msg.ping_event?.ping_ms ?? 0
        );
        break;
      case "audio": {
        const b64 = msg.audio_event?.audio_base_64;
        if (b64) this.playPcm16(base64ToPcm16(b64));
        break;
      }
      case "interruption":
        this.stopPlayback(); // user barged in
        break;
      case "agent_response":
        this.opts.onAgentText?.(msg.agent_response_event?.agent_response ?? "");
        break;
    }
  }

  private playPcm16(samples: Float32Array) {
    if (!this.ctx) return;
    const rate = this.opts.agentOutputRate ?? 16000;
    const buf = this.ctx.createBuffer(1, samples.length, rate);
    buf.getChannelData(0).set(samples);
    const node = this.ctx.createBufferSource();
    node.buffer = buf;
    node.connect(this.ctx.destination);
    const now = this.ctx.currentTime;
    this.playHead = Math.max(this.playHead, now);
    node.start(this.playHead);
    this.playHead += buf.duration;
    node.onended = () => {
      this.sources = this.sources.filter((s) => s !== node);
    };
    this.sources.push(node);
  }

  private stopPlayback() {
    for (const s of this.sources) { try { s.stop(); } catch {} }
    this.sources = [];
    this.playHead = 0;
  }
}

// --- PCM16 / base64 helpers -------------------------------------------------

export function f32ToBase64Pcm16(f32: Float32Array): string {
  const pcm = new Uint8Array(f32.length * 2);
  const view = new DataView(pcm.buffer);
  for (let i = 0; i < f32.length; i++) {
    const s = Math.max(-1, Math.min(1, f32[i]));
    view.setInt16(i * 2, (s * 32767) | 0, true);
  }
  let bin = "";
  for (let i = 0; i < pcm.length; i++) bin += String.fromCharCode(pcm[i]);
  return btoa(bin);
}

export function base64ToPcm16(b64: string): Float32Array {
  const bin = atob(b64);
  const n = bin.length >> 1;
  const out = new Float32Array(n);
  const view = new DataView(new ArrayBuffer(2));
  for (let i = 0; i < n; i++) {
    view.setUint8(0, bin.charCodeAt(i * 2));
    view.setUint8(1, bin.charCodeAt(i * 2 + 1));
    out[i] = view.getInt16(0, true) / 32768;
  }
  return out;
}
