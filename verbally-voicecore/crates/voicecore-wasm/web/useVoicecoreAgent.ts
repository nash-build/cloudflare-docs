// React hook wrapper around VoicecoreAgent — drop into a Lovable / Vite + React
// app. Adjust the wasm import path to match your published/colocated package.

import { useCallback, useEffect, useRef, useState } from "react";
import { VoicecoreAgent, type VoicecoreEngine } from "./voicecore-agent";

// TODO: point this at your wasm-pack output (npm package or local path).
//   import init, { WasmEngine } from "@verbally/voicecore-wasm";
// For a quick test you can colocate the pkg/ folder and import from it:
import init, { WasmEngine } from "./pkg/voicecore_wasm.js";

let wasmReady: Promise<void> | null = null;
function ensureWasm(): Promise<void> {
  if (!wasmReady) wasmReady = init().then(() => undefined);
  return wasmReady;
}

async function createEngine(sampleRate: number): Promise<VoicecoreEngine> {
  await ensureWasm();
  return new WasmEngine(sampleRate) as unknown as VoicecoreEngine;
}

export interface UseVoicecoreAgentOptions {
  agentId?: string;
  /** Calls your backend (e.g. Supabase edge function) for a signed URL. */
  getSignedUrl?: () => Promise<string>;
  profile?: Uint8Array;
  workletUrl?: string;
}

export function useVoicecoreAgent(opts: UseVoicecoreAgentOptions) {
  const agentRef = useRef<VoicecoreAgent | null>(null);
  const [status, setStatus] = useState<"idle" | "connecting" | "live" | "stopped">("idle");
  const [agentText, setAgentText] = useState("");
  const [error, setError] = useState<unknown>(null);

  const start = useCallback(async () => {
    if (agentRef.current) return;
    const agent = new VoicecoreAgent({
      createEngine,
      agentId: opts.agentId,
      getSignedUrl: opts.getSignedUrl,
      profile: opts.profile,
      workletUrl: opts.workletUrl,
      onStatus: setStatus,
      onAgentText: setAgentText,
      onError: setError,
    });
    agentRef.current = agent;
    try {
      await agent.start();
    } catch (e) {
      setError(e);
      agentRef.current = null;
    }
  }, [opts.agentId, opts.getSignedUrl, opts.profile, opts.workletUrl]);

  const stop = useCallback(() => {
    agentRef.current?.stop();
    agentRef.current = null;
  }, []);

  useEffect(() => () => agentRef.current?.stop(), []);

  return { start, stop, status, agentText, error };
}
