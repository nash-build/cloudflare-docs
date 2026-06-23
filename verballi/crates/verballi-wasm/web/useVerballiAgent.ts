// React hook wrapper around VerballiAgent — drop into a Lovable / Vite + React
// app. Adjust the wasm import path to match your published/colocated package.

import { useCallback, useEffect, useRef, useState } from "react";
import { VerballiAgent, type VerballiEngine } from "./verballi-agent";

// TODO: point this at your wasm-pack output (npm package or local path).
//   import init, { WasmEngine } from "@verballi/verballi-wasm";
// For a quick test you can colocate the pkg/ folder and import from it:
import init, { WasmEngine } from "./pkg/verballi_wasm.js";

let wasmReady: Promise<void> | null = null;
function ensureWasm(): Promise<void> {
  if (!wasmReady) wasmReady = init().then(() => undefined);
  return wasmReady;
}

async function createEngine(sampleRate: number): Promise<VerballiEngine> {
  await ensureWasm();
  return new WasmEngine(sampleRate) as unknown as VerballiEngine;
}

export interface UseVerballiAgentOptions {
  agentId?: string;
  /** Calls your backend (e.g. Supabase edge function) for a signed URL. */
  getSignedUrl?: () => Promise<string>;
  profile?: Uint8Array;
  workletUrl?: string;
}

export function useVerballiAgent(opts: UseVerballiAgentOptions) {
  const agentRef = useRef<VerballiAgent | null>(null);
  const [status, setStatus] = useState<"idle" | "connecting" | "live" | "stopped">("idle");
  const [agentText, setAgentText] = useState("");
  const [error, setError] = useState<unknown>(null);

  const start = useCallback(async () => {
    if (agentRef.current) return;
    const agent = new VerballiAgent({
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
