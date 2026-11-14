import { useState } from "react";
import type { Client } from "@connectrpc/connect";
import { hostNotConnectedText, rpcFailureText } from "tddy-client-web";
import type { SessionService } from "tddy-client-web/gen/session_pb";
import type { StartSessionRequestInit } from "./hooks/useSessionAttachments";

/** What the orchestrator composer's send does, and what it shows while and after it does it. */
export interface OrchestratorComposerStart {
  /**
   * Sends `StartSession` with `request` and hands the started session's id to `onStarted`. A
   * refusal never rejects: it lands in `error`, and the composer keeps its text.
   */
  start: (request: StartSessionRequestInit) => Promise<void>;
  /** A start is in flight: the send is disabled until it settles. */
  starting: boolean;
  /** The daemon's reason for the last refused start; `null` before one and after a success. */
  error: string | null;
}

/**
 * Starts the orchestrator the composer describes (`#orchestrators` 8/11) over `client` — the
 * selected daemon's `SessionService`; the request's `daemon_instance_id` names the host it runs on.
 * `null` until a daemon is reachable: a start then fails with the host's absence as its reason.
 */
export function useOrchestratorComposerStart(
  client: Client<typeof SessionService> | null,
  onStarted: (sessionId: string) => void,
): OrchestratorComposerStart {
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const start = async (request: StartSessionRequestInit): Promise<void> => {
    if (!client) {
      setError(hostNotConnectedText(null));
      return;
    }
    setStarting(true);
    setError(null);
    try {
      const res = await client.startSession(request);
      onStarted(res.sessionId);
    } catch (err) {
      setError(rpcFailureText(err));
    } finally {
      setStarting(false);
    }
  };

  return { start, starting, error };
}
