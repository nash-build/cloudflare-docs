// AudioWorklet capture tap for voicecore.
//
// It does no DSP itself — it just buffers mic frames to ~128 ms blocks and posts
// them to the main thread, where the WASM engine isolates the voice. (Running
// the engine on the main thread avoids the complexity of loading WASM inside the
// AudioWorklet scope, and the block size keeps message traffic low.)
//
// Place this file where your app serves static assets (e.g. Vite/Lovable
// `public/`) and load it with `audioWorklet.addModule('/capture-worklet.js')`.

class VoicecoreCapture extends AudioWorkletProcessor {
  constructor() {
    super();
    this._buf = [];
    this._target = 2048; // ~128 ms @ 16 kHz-equiv; tune for latency vs overhead
  }

  process(inputs) {
    const input = inputs[0];
    if (input && input[0]) {
      const ch = input[0];
      for (let i = 0; i < ch.length; i++) this._buf.push(ch[i]);
      if (this._buf.length >= this._target) {
        const out = Float32Array.from(this._buf);
        this.port.postMessage(out, [out.buffer]);
        this._buf = [];
      }
    }
    return true; // keep processor alive
  }
}

registerProcessor("voicecore-capture", VoicecoreCapture);
