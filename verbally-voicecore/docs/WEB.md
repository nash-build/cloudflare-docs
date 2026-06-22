# Browser / Electron (WebAssembly)

`voicecore-wasm` compiles the engine to WebAssembly so you can isolate the mic
**in the browser** and hand cleaned audio to the ElevenLabs JS SDK — the natural
setup for a web voice agent.

## Build

```bash
# one-time
cargo install wasm-pack

# build the JS package
wasm-pack build crates/voicecore-wasm --target web --release
# → crates/voicecore-wasm/pkg/  (voicecore_wasm.js + voicecore_wasm_bg.wasm)
```

(The core has no OS dependencies, so it builds to `wasm32-unknown-unknown` as-is;
`scripts/build-wasm.sh` wraps the command. Run `wasm-opt -Oz` on the `.wasm` to
shrink it further.)

## Wire it into an AudioWorklet

Do the DSP off the main thread. The worklet processor calls the engine on each
128-sample render quantum:

```js
// worklet.js
import init, { WasmEngine } from "./pkg/voicecore_wasm.js";

class IsolateProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.ready = false;
    init().then(() => {
      this.engine = new WasmEngine(sampleRate); // AudioWorklet global
      this.ready = true;
    });
  }
  process(inputs, outputs) {
    if (!this.ready) return true;
    const inCh = inputs[0][0];
    if (inCh) {
      const clean = this.engine.process(inCh);     // Float32Array @ 16 kHz
      this.port.postMessage(clean, [clean.buffer]); // → main thread → ElevenLabs
    }
    return true;
  }
}
registerProcessor("isolate", IsolateProcessor);
```

```js
// main.js
const ctx = new AudioContext();
await ctx.audioWorklet.addModule("worklet.js");
const mic = await navigator.mediaDevices.getUserMedia({ audio: true });
const src = ctx.createMediaStreamSource(mic);
const node = new AudioWorkletNode(ctx, "isolate");
src.connect(node);
node.port.onmessage = (e) => sendToElevenLabs(e.data); // cleaned 16 kHz PCM
```

## Feed ElevenLabs

Use ElevenLabs' own JS SDK (`@elevenlabs/client` / `@elevenlabs/react`) for the
agent connection; the worklet's cleaned output is your input track. This keeps
the clean-room split: **our** WASM makes clean audio, **their** SDK transports it.

## Enrollment & profiles

```js
engine.begin_enrollment();
// feed ~20–30 s of the user's voice through process()...
const profile = engine.finish_enrollment();   // Uint8Array
localStorage.setItem("voiceProfile", btoa(String.fromCharCode(...profile)));

// later:
const saved = Uint8Array.from(atob(localStorage.getItem("voiceProfile")), c => c.charCodeAt(0));
engine.set_profile(saved);
```

## Notes

- The engine outputs 16 kHz mono regardless of the `AudioContext` rate you pass
  to the constructor; resample to the agent's expected rate in JS if needed.
- For the strongest identity lock, run an ECAPA ONNX model with
  `onnxruntime-web` on a rolling window and call `engine.set_speaker_presence(score)`
  — mirrors the native `PresenceScorer` loop (see `NEURAL_MODELS.md`).
