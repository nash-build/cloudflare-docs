# Testing in a Lovable (browser/React) SaaS app

Yes — this works in a Lovable app, and it's a good fit. Lovable builds a
React + Vite app that runs in the browser, which is exactly what `verbally-wasm`
+ the web kit target. The engine runs client-side; your app talks to the
ElevenLabs agent directly over WebSocket with **isolated** audio.

The drop-in kit lives in `crates/verbally-wasm/web/`:
`capture-worklet.js`, `verbally-agent.ts`, `useVerballyAgent.ts`.

## One hard rule: keep your ElevenLabs API key server-side

Never ship `ELEVENLABS_API_KEY` in a browser app. For a **public** agent you can
connect with just the `agent_id`. For a **private** agent, your app calls a tiny
backend endpoint that returns a *signed URL*. Lovable's Supabase integration is
the easy way to host that (see the edge function below).

## Steps

### 1. Build the WASM package
On your machine (not in Lovable):
```bash
cargo install wasm-pack            # once
wasm-pack build crates/verbally-wasm --target web --release
# → crates/verbally-wasm/pkg/  (verbally_wasm.js + verbally_wasm_bg.wasm + .d.ts)
```
Two ways to consume it in Lovable:
- **Publish to npm** (recommended): `cd pkg && npm publish --access public`, then
  in Lovable `npm i @your-scope/verbally-wasm` and import from it.
- **Colocate**: copy `pkg/` into your app's `src/` and import from `./pkg/...`.
  Put `verbally_wasm_bg.wasm` where Vite serves assets.

### 2. Add the capture worklet to `public/`
Copy `capture-worklet.js` into the app's `public/` folder so it's served at
`/capture-worklet.js` (the default the kit loads).

### 3. Add the kit files
Copy `verbally-agent.ts` and `useVerballyAgent.ts` into `src/`. In
`useVerballyAgent.ts`, fix the import to your package/path:
```ts
import init, { WasmEngine } from "@your-scope/verbally-wasm"; // or "./pkg/verbally_wasm.js"
```

### 4. Use it in a component
```tsx
import { useVerballyAgent } from "./useVerballyAgent";

export function VoiceButton() {
  const { start, stop, status, agentText } = useVerballyAgent({
    // Public agent:
    agentId: import.meta.env.VITE_ELEVENLABS_AGENT_ID,
    // Private agent instead:
    // getSignedUrl: async () => (await fetch("/api/signed-url").then(r => r.json())).signed_url,
  });
  return (
    <div>
      <button onClick={status === "live" ? stop : start}>
        {status === "live" ? "Stop" : "Talk to agent"}
      </button>
      <p>{status}</p>
      <p>{agentText}</p>
    </div>
  );
}
```

### 5. (Private agents) Supabase edge function for the signed URL
```ts
// supabase/functions/signed-url/index.ts
Deno.serve(async () => {
  const agentId = Deno.env.get("ELEVENLABS_AGENT_ID")!;
  const r = await fetch(
    `https://api.elevenlabs.io/v1/convai/conversation/get-signed-url?agent_id=${agentId}`,
    { headers: { "xi-api-key": Deno.env.get("ELEVENLABS_API_KEY")! } }
  );
  return new Response(await r.text(), { headers: { "content-type": "application/json" } });
});
```
Point `getSignedUrl` at this function's URL.

### 6. Enrollment (optional but recommended for "keep only me")
Record ~20–30 s of the user, then:
```ts
const agent = new VerballyAgent({ createEngine, agentId });
const profile = await agent.enroll(voiceFloat32, audioContext.sampleRate); // Uint8Array
localStorage.setItem("voiceProfile", btoa(String.fromCharCode(...profile)));
// next session: pass `profile` (decoded) into useVerballyAgent
```

## Gotchas to expect in the browser

- **Agent audio format:** the kit assumes the agent outputs `pcm_16000`
  (ElevenLabs default). If you changed it, set `agentOutputRate`.
- **Input format:** the engine emits 16 kHz mono PCM16 — ElevenLabs agents accept
  this by default. No change needed unless you set a non-default input format.
- **Autoplay:** browsers require a user gesture before audio; `start()` is called
  from a click, which satisfies this.
- **WASM MIME type:** ensure `.wasm` is served as `application/wasm` (Vite and
  Lovable's hosting do this by default).
- **Echo:** `getUserMedia` requests `echoCancellation: true` and disables the
  browser's own noise suppression/AGC so verbally is the only thing cleaning the
  signal. If you hear the agent looping back, keep echoCancellation on (it is).

## What still needs your assets

- Your **ElevenLabs `agent_id`** (and API key in the Supabase secret for private).
- For the strongest identity lock, an **ECAPA ONNX** model run via
  `onnxruntime-web` on a rolling window, calling `engine.set_speaker_presence()`
  — mirrors the native `PresenceScorer` (see `NEURAL_MODELS.md`). The built-in
  MFCC lock works without it.
