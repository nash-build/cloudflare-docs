# Verballi Classroom SDK — Implementation Guide (for the IT / dev team)

This is the end-to-end runbook to get the SDK live in a **Lovable** (or any
Vite + React) app, talking to an **ElevenLabs** voice agent with on-device voice
isolation. The WebAssembly engine is **prebuilt** in `web/pkg/` — no Rust needed.

There are two integration modes:
- **Mode A — copy files (recommended for the first test).** Zero build config.
- **Mode B — npm package.** Publish the engine to npm and `npm install` it.

---

## 0. Prerequisites
- This bundle, unzipped.
- An ElevenLabs account with an **Agent** created (note its **agent id**).
- The Lovable project (or a local Vite + React app).

## 1. Configure the ElevenLabs agent
1. ElevenLabs dashboard → **Agents** → your agent.
2. **Audio output format = PCM 16000 Hz** (the SDK default). If different, you'll
   set `agentOutputRate` in the kit.
3. For the first test, make the agent **public** (allow unauthenticated web
   access) and copy the **agent id** (`agent_xxxx`). Private agents are step 6.

## 2. Smoke test (2 min, no build — proves the whole chain)
```bash
cd web
npx serve .            # or: python3 -m http.server 8080
```
Open the served URL → paste the public agent id → **Start** → allow mic → talk.
✅ You hear the agent reply and your mic is cleaned. Fix any issue here before Lovable.

---

## Mode A — copy files into the app (recommended)

### 3A. Place the files
- `web/pkg/` → `src/lib/verballi/pkg/`
- `web/verballi-agent.ts`, `web/useVerballiAgent.ts` → `src/lib/verballi/`
- `web/capture-worklet.js` → **`public/capture-worklet.js`** (served at `/capture-worklet.js`)

### 4A. Confirm the engine import
In `src/lib/verballi/useVerballiAgent.ts`:
```ts
import init, { WasmEngine } from "./pkg/verballi_wasm.js";
```
(correct if `pkg/` sits next to the hook, as above.)

---

## Mode B — npm package (optional)

### 3B. Publish the engine
```bash
cd web/pkg
npm publish --access public      # publishes "verballi-classroomsdk"
```
(For a private registry/scope, set `name` in `web/pkg/package.json` accordingly,
e.g. `@your-scope/verballi-classroomsdk`, and `npm publish`.)

### 4B. Install + wire
```bash
npm install verballi-classroomsdk
```
Still copy `verballi-agent.ts` + `useVerballiAgent.ts` into `src/` (they're thin
glue), and change the engine import at the top of `useVerballiAgent.ts`:
```ts
import init, { WasmEngine } from "verballi-classroomsdk";
```
`capture-worklet.js` still goes in `public/`.

---

## 5. Add the component & agent id
- Lovable env var: `VITE_ELEVENLABS_AGENT_ID = agent_xxxx`
- Use `web/example-Conversation.tsx` (copy into `src/`):
```tsx
import { useVerballiAgent } from "@/lib/verballi/useVerballiAgent";

export function Conversation() {
  const { start, stop, status, agentText } = useVerballiAgent({
    agentId: import.meta.env.VITE_ELEVENLABS_AGENT_ID,
  });
  return (
    <div>
      <button onClick={status === "live" ? stop : start}>
        {status === "live" ? "Stop" : "Talk"}
      </button>
      <p>{status}</p><p>{agentText}</p>
    </div>
  );
}
```
Run the Lovable preview → click Talk → allow mic. Live on a public agent.

## 6. Private agent (production — keep the API key server-side)
**Never put `ELEVENLABS_API_KEY` in the browser.** Add a Supabase edge function:
```ts
// supabase/functions/signed-url/index.ts
Deno.serve(async () => {
  const id = Deno.env.get("ELEVENLABS_AGENT_ID")!;
  const r = await fetch(
    `https://api.elevenlabs.io/v1/convai/conversation/get-signed-url?agent_id=${id}`,
    { headers: { "xi-api-key": Deno.env.get("ELEVENLABS_API_KEY")! } }
  );
  return new Response(await r.text(), { headers: { "content-type": "application/json" }});
});
```
Set Supabase secrets `ELEVENLABS_API_KEY`, `ELEVENLABS_AGENT_ID`, then:
```tsx
useVerballiAgent({
  getSignedUrl: async () =>
    (await fetch("/functions/v1/signed-url").then(r => r.json())).signed_url,
});
```

## 7. (Optional) Enroll the speaker — strongest "keep only this person"
```ts
// record ~20–30 s of the user into a Float32Array `voice`, then:
const agent = new VerballiAgent({ createEngine, agentId });
const profile = await agent.enroll(voice, audioContext.sampleRate); // Uint8Array
localStorage.setItem("voiceProfile", btoa(String.fromCharCode(...profile)));
// next session: decode and pass `profile` into useVerballiAgent({ ..., profile })
```

## 8. Go-live checklist
- [ ] Served over **HTTPS** (mic + WASM need a secure context; Lovable hosting is fine).
- [ ] `.wasm` served as `application/wasm` (Vite/Lovable default).
- [ ] `capture-worklet.js` reachable at `/capture-worklet.js`.
- [ ] Private agent → API key only in Supabase secrets, never client-side.
- [ ] Agent output format = `pcm_16000` (or set `agentOutputRate`).
- [ ] `echoCancellation` stays on (the kit requests it) to avoid feedback.

## Troubleshooting
| Symptom | Likely cause / fix |
|---------|--------------------|
| Connects then drops after ~20 s | (kit handles ping/pong) check agent output format is PCM 16000 |
| No agent audio | check the agent is published/public, or signed URL is valid |
| Robotic / wrong-speed playback | agent output isn't 16 kHz → set `agentOutputRate` |
| Mic blocked | must be HTTPS/localhost and triggered by a click (Start button) |
| `capture-worklet.js` 404 | file isn't in `public/` |
| Worklet/WASM CORS errors | serve same-origin; don't load pkg from a cross-origin URL |

## What still needs your assets
- ElevenLabs **agent id** (+ API key in Supabase secret for private agents).
- For the strongest identity lock in noisy rooms, an **ECAPA ONNX** model run via
  `onnxruntime-web` on a rolling window, calling `engine.set_speaker_presence()`
  (see `NEURAL_MODELS.md`). The built-in MFCC lock works without it.
