import React, { useState, useEffect, useCallback, useRef } from "react";
import type { Client } from "@connectrpc/connect";
import { ConnectionService } from "../../gen/connection_pb";
import { useDaemonClient } from "../../rpc/selectedDaemon";
import { getScreenId } from "../../lib/screenId";
import {
  type TerminalControlState,
  initialTerminalControlState,
  applyTerminalControlEvent,
} from "./terminalControlState";

export interface UseTerminalControlResult {
  controlState: TerminalControlState;
  /** Ref to the control token granted by ClaimTerminalControl. Empty string until granted. */
  controlTokenRef: React.MutableRefObject<string>;
  /** Steal control from the current holder (steal=true ClaimTerminalControl). */
  claim(): Promise<void>;
  /** Reset state (call when detaching from a session). */
  reset(): void;
}

type ControlClient = Client<typeof ConnectionService>;

async function runControlSession(
  sessionId: string,
  sessionToken: string,
  screenId: string,
  client: ControlClient,
  controlTokenRef: { current: string },
  onState: (s: TerminalControlState) => void,
  signal: AbortSignal,
): Promise<void> {
  const resp = await client.claimTerminalControl({
    sessionToken,
    sessionId,
    screenId,
    steal: false,
  });
  if (resp.granted) {
    controlTokenRef.current = resp.controlToken;
    onState({ isController: true, holderScreenId: screenId });
  } else {
    onState({ isController: false, holderScreenId: resp.currentHolderScreenId });
  }

  for await (const event of client.watchTerminalControl(
    { sessionToken, sessionId, controlToken: controlTokenRef.current },
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

export function useTerminalControl(
  sessionId: string | null,
  sessionToken: string,
  /**
   * Optional lazy builder for a session-scoped `ConnectionService` client (targets the coder
   * participant `daemon-{instanceId}-{sessionId}`). When provided, the explicit `claim()` (the
   * "Claim terminal" button, steal=true) routes through it. The auto-claim-on-attach (steal=false)
   * always uses the daemon client below so control is still acquired automatically when no one else
   * holds the lease. `null`/`undefined` falls back to the daemon client for both paths.
   */
  buildSessionClient?: () => Client<typeof ConnectionService> | null,
): UseTerminalControlResult {
  // ConnectionService is daemon-level RPC — routed to the currently selected daemon (see
  // `SelectedDaemonProvider`). `null` until a daemon is selected / the room is connected. Used for
  // the auto-claim-on-attach (steal=false) and as the fallback for the explicit claim.
  const client = useDaemonClient(ConnectionService);
  const [controlState, setControlState] = useState<TerminalControlState>(
    initialTerminalControlState,
  );
  const controlTokenRef = useRef<string>("");
  const screenId = getScreenId();

  // On session attach: claim control (steal=false) then subscribe to lease changes.
  useEffect(() => {
    if (!sessionId || !client) return;
    const abortController = new AbortController();

    void runControlSession(
      sessionId,
      sessionToken,
      screenId,
      client,
      controlTokenRef,
      setControlState,
      abortController.signal,
    ).catch(() => {
      // AbortError on cleanup, or stream ended — expected
    });

    return () => abortController.abort();
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
        controlTokenRef.current = resp.controlToken;
        setControlState({ isController: true, holderScreenId: screenId });
      }
    } catch {
      // network/auth error — state remains isController:false, user may retry
    }
  }, [sessionId, sessionToken, client, screenId, buildSessionClient]);

  const reset = useCallback(() => {
    controlTokenRef.current = "";
    setControlState(initialTerminalControlState);
  }, []);

  return { controlState, controlTokenRef, claim, reset };
}
