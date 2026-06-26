import React, { useState, useEffect, useCallback, useRef } from "react";
import type { Client } from "@connectrpc/connect";
import { ConnectionService } from "../../gen/connection_pb";
import { useHttpClient } from "../../rpc/transportProvider";
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
): UseTerminalControlResult {
  const client = useHttpClient(ConnectionService);
  const [controlState, setControlState] = useState<TerminalControlState>(
    initialTerminalControlState,
  );
  const controlTokenRef = useRef<string>("");
  const screenId = getScreenId();

  // On session attach: claim control (steal=false) then subscribe to lease changes.
  useEffect(() => {
    if (!sessionId) return;
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
    try {
      const resp = await client.claimTerminalControl({
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
  }, [sessionId, sessionToken, client, screenId]);

  const reset = useCallback(() => {
    controlTokenRef.current = "";
    setControlState(initialTerminalControlState);
  }, []);

  return { controlState, controlTokenRef, claim, reset };
}
