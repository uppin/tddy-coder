import { useState, useEffect, useCallback } from "react";
import type { Client } from "@connectrpc/connect";
import { ConnectionService } from "../../gen/connection_pb";
import { getScreenId } from "../../lib/screenId";
import {
  type TerminalControlState,
  initialTerminalControlState,
  applyTerminalControlEvent,
} from "./terminalControlState";

// ---------------------------------------------------------------------------
// Session vs ConnectedSession
// ---------------------------------------------------------------------------

type ControlClient = Client<typeof ConnectionService>;

/**
 * A reference to a session addressed via the daemon's LiveKit-RPC participant (`daemon-{instanceId}`).
 * Carries the daemon `ConnectionService` client used for the daemon-served terminal **output** stream
 * (`streamTerminalOutput`, no token) and as the auto-claim target. You cannot send terminal input on
 * a bare `Session` — input requires a {@link ConnectedSession} (the claim's lease).
 */
export interface Session {
  sessionId: string;
  client: ControlClient;
}

/**
 * A {@link Session} that has been claimed for control — carries the lease token granted by
 * `ClaimTerminalControl`. Produced *only* by a successful claim; `null` while the claim is in flight
 * or was denied. Terminal input (`sendTerminalInput`) is callable exclusively on a `ConnectedSession`,
 * so input can never race the claim: there is no `ConnectedSession` to send with until the claim
 * resolves. For sessions with a LiveKit-room participant, the explicit steal-claim ("Claim terminal"
 * button) still routes through the session participant via `buildSessionClient`; the token returned
 * is the same lease regardless of which participant served the claim.
 */
export interface ConnectedSession {
  sessionId: string;
  controlToken: string;
}

// ---------------------------------------------------------------------------
// Hook result
// ---------------------------------------------------------------------------

export interface UseTerminalControlResult {
  controlState: TerminalControlState;
  /** The claimed session (lease token in hand), or `null` while the claim is in flight / denied.
   *  Reactive state — the terminal re-renders when the claim resolves, so input gating and the
   *  flush of queued input are driven by this object, not a ref. */
  connected: ConnectedSession | null;
  /** Steal control from the current holder (steal=true ClaimTerminalControl). */
  claim(): Promise<void>;
  /** Reset state (call when detaching from a session). */
  reset(): void;
}

// ---------------------------------------------------------------------------
// Claim loop
// ---------------------------------------------------------------------------

async function runControlSession(
  sessionId: string,
  sessionToken: string,
  screenId: string,
  client: ControlClient,
  onConnected: (connected: ConnectedSession) => void,
  onState: (s: TerminalControlState) => void,
  signal: AbortSignal,
): Promise<void> {
  const resp = await client.claimTerminalControl({
    sessionToken,
    sessionId,
    screenId,
    steal: false,
  });
  const controlToken = resp.granted ? resp.controlToken : "";
  if (resp.granted) {
    onConnected({ sessionId, controlToken });
    onState({ isController: true, holderScreenId: screenId });
  } else {
    onState({ isController: false, holderScreenId: resp.currentHolderScreenId });
  }

  for await (const event of client.watchTerminalControl(
    { sessionToken, sessionId, controlToken },
    { signal },
  )) {
    onState(
      applyTerminalControlEvent(initialTerminalControlState, {
        holderScreenId: event.holderScreenId,
        youAreController: event.youAreController,
      }),
    );
  }
}

// ---------------------------------------------------------------------------
// Hook
// ---------------------------------------------------------------------------

export function useTerminalControl(
  /** The session to claim control for. `null` until the owning daemon client is reachable; the
   *  effect waits until a `Session` is available. Pass the owning daemon's client (not the
   *  selected-daemon client) so a cross-host session's lease is acquired on the host that owns it. */
  session: Session | null,
  sessionToken: string,
  /** Optional lazy builder for a session-scoped `ConnectionService` client (targets the coder
   *  participant `daemon-{instanceId}-{sessionId}`). When provided, the explicit `claim()` (the
   *  "Claim terminal" button, steal=true) routes through it. The auto-claim-on-attach (steal=false)
   *  always uses the daemon `Session.client` so control is acquired automatically when no one else
   *  holds the lease. `null`/`undefined` falls back to the daemon client for both paths. */
  buildSessionClient?: () => ControlClient | null,
): UseTerminalControlResult {
  const sessionId = session?.sessionId ?? null;
  const client = session?.client ?? null;

  const [controlState, setControlState] = useState<TerminalControlState>(
    initialTerminalControlState,
  );
  const [connected, setConnected] = useState<ConnectedSession | null>(null);
  const screenId = getScreenId();

  // On session attach (or when the owning client becomes available): drop any stale lease from a
  // previous session so the new session's terminal input never leaks the previous session's token,
  // then claim control (steal=false) and subscribe to lease changes. `connected` stays null until
  // the claim resolves — the terminal gates input on it.
  useEffect(() => {
    if (!sessionId || !client) return;
    setConnected(null);
    setControlState(initialTerminalControlState);
    const abortController = new AbortController();

    void runControlSession(
      sessionId,
      sessionToken,
      screenId,
      client,
      setConnected,
      setControlState,
      abortController.signal,
    ).catch(() => {
      // AbortError on cleanup, or stream ended — expected
    });

    return () => abortController.abort();
    // `session` is intentionally decomposed into `sessionId`/`client` so the effect is keyed on the
    // stable primitives, not the `Session` object identity (which the caller rebuilds each render).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, sessionToken, client, screenId]);

  const claim = useCallback(async () => {
    if (!sessionId) return;
    // Route the explicit steal-claim through the session participant when a session-scoped client
    // is available; otherwise fall back to the daemon participant.
    const claimClient = buildSessionClient?.() ?? client;
    if (!claimClient) return;
    try {
      const resp = await claimClient.claimTerminalControl({
        sessionToken,
        sessionId,
        screenId,
        steal: true,
      });
      if (resp.granted) {
        setConnected({ sessionId, controlToken: resp.controlToken });
        setControlState({ isController: true, holderScreenId: screenId });
      }
    } catch {
      // network/auth error — state remains isController:false, user may retry
    }
  }, [sessionId, sessionToken, client, screenId, buildSessionClient]);

  const reset = useCallback(() => {
    setConnected(null);
    setControlState(initialTerminalControlState);
  }, []);

  return { controlState, connected, claim, reset };
}
