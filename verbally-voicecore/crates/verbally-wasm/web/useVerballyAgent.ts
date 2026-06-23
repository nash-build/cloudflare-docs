// React hook wrapper around VerballyAgent — drop into a Lovable / Vite + React
// app. Adjust the wasm import path to match your published/colocated package.

import { useCallback, useEffect, useRef, useState } from "react";
import { VerballyAgent, type VerballyEngine } from "./verbally-agent";

// TODO: point this at your wasm-pack output (npm package or local path).
//   import init, { WasmEngine } from "@verbally/verbally-wasm";
// For a quick test you can colocate the pkg/ folder and import from it:
import init, { WasmEngine } from "./pkg/verbally_wasm.js";

let wasmReady: Promise<void> | null = null;
function ensureWasm(): Promise<void> {
  if (!wasmReady) wasmReady = init().then(() => undefined);
  return wasmReady;
}

async function createEngine(sampleRate: number): Promise<VerballyEngine> {
  await ensureWasm();
  return new WasmEngine(sampleRate) as unknown as VerballyEngine;
}

export interface UseVerballyAgentOptions {
  agentId?: string;
  /** Calls your backend (e.g. Supabase edge function) for a signed URL. */
  getSignedUrl?: () => Promise<string>;
  profile?: Uint8Array;
  workletUrl?: string;
}

export function useVerballyAgent(opts: UseVerballyAgentOptions) {
  const agentRef = useRef<VerballyAgent | null>(null);
  const [status, setStatus] = useState<"idle" | "connecting" | "live" | "stopped">("idle");
  const [agentText, setAgentText] = useState("");
  const [error, setError] = useState<unknown>(null);

  const start = useCallback(async () => {
    if (agentRef.current) return;
    const agent = new VerballyAgent({
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
