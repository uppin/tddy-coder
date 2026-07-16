import { useCallback, useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { ConnectionService } from "../../gen/connection_pb";
import { tddyDebug } from "../../lib/debugMask";

const dTerm = tddyDebug("tddy:term:tabs");

type ConnectionClient = Client<typeof ConnectionService>;

/** The reserved main terminal id — the coding Agent tab. Never handed out by StartTerminalSession. */
export const AGENT_TERMINAL_ID = "main";

export interface SessionTerminalsController {
  /** Open bash terminals for this session, in open order (the Agent/`main` terminal is implicit). */
  readonly terminals: string[];
  /** The currently focused terminal id — `AGENT_TERMINAL_ID` or one of `terminals`. */
  readonly activeTerminalId: string;
  /** Focus an existing terminal tab. */
  setActive: (terminalId: string) => void;
  /** Start a new bash terminal, append its tab, and focus it. */
  open: () => void;
  /** Stop a bash terminal, remove its tab, and fall back to the Agent tab if it was focused. */
  close: (terminalId: string) => void;
  /** Drop a bash tab whose terminal has already ended (its output stream closed) — no Stop RPC. */
  dropEnded: (terminalId: string) => void;
}

export interface UseSessionTerminalsParams {
  sessionId: string;
  sessionToken: string;
  /** Client used for the terminal RPCs (the daemon client for gRPC sessions, the session-scoped
   *  client for LiveKit sessions). `null` until it is available; listing waits for it. */
  client: ConnectionClient | null;
  /** Current terminal-control lease token, forwarded to Start/Stop when a controller is active. */
  controlToken?: string;
}

/**
 * Owns the set of bash terminals for a single session plus the active-tab selection.
 *
 * On attach it lists the session's already-open bash terminals; `open`/`close` drive the daemon
 * `StartTerminalSession`/`StopTerminalSession` RPCs and keep the local tab set consistent with an
 * optimistic remove (the tab disappears immediately, the Stop RPC fires in the background). The
 * reserved `main` terminal (the Agent tab) is implicit and is never part of `terminals`.
 */
export function useSessionTerminals({
  sessionId,
  sessionToken,
  client,
  controlToken = "",
}: UseSessionTerminalsParams): SessionTerminalsController {
  const [terminals, setTerminals] = useState<string[]>([]);
  const [activeTerminalId, setActiveTerminalId] = useState<string>(AGENT_TERMINAL_ID);

  // Populate the bash-terminal tabs from the session's current terminals once a client is available.
  useEffect(() => {
    if (!client) return;
    let cancelled = false;
    void (async () => {
      try {
        const res = await client.listTerminalSessions({ sessionToken, sessionId });
        if (cancelled) return;
        const bash = res.terminals
          .map((t) => t.terminalId)
          .filter((id) => id && id !== AGENT_TERMINAL_ID);
        setTerminals((prev) => {
          // Merge: keep any locally-opened tab not yet reflected by the daemon list.
          const merged = [...bash];
          for (const id of prev) if (!merged.includes(id)) merged.push(id);
          return merged;
        });
      } catch (err) {
        dTerm(
          "listTerminalSessions failed sessionId=%s error=%o",
          sessionId,
          err instanceof Error ? err.message : err,
        );
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client, sessionId, sessionToken]);

  const setActive = useCallback((id: string) => {
    setActiveTerminalId(id);
  }, []);

  const open = useCallback(() => {
    if (!client) return;
    void (async () => {
      try {
        const res = await client.startTerminalSession({ sessionToken, sessionId, controlToken });
        const id = res.terminalId;
        if (!id) return;
        setTerminals((prev) => (prev.includes(id) ? prev : [...prev, id]));
        setActiveTerminalId(id);
      } catch (err) {
        dTerm(
          "startTerminalSession failed sessionId=%s error=%o",
          sessionId,
          err instanceof Error ? err.message : err,
        );
      }
    })();
  }, [client, sessionId, sessionToken, controlToken]);

  const dropEnded = useCallback((id: string) => {
    setTerminals((prev) => prev.filter((t) => t !== id));
    setActiveTerminalId((prev) => (prev === id ? AGENT_TERMINAL_ID : prev));
  }, []);

  const close = useCallback(
    (id: string) => {
      // Optimistic remove — the tab and its pane come down immediately; the Stop RPC follows.
      dropEnded(id);
      if (!client) return;
      void client
        .stopTerminalSession({ sessionToken, sessionId, terminalId: id, controlToken })
        .catch((err) => {
          dTerm(
            "stopTerminalSession failed sessionId=%s terminalId=%s error=%o",
            sessionId,
            id,
            err instanceof Error ? err.message : err,
          );
        });
    },
    [client, sessionId, sessionToken, controlToken, dropEnded],
  );

  return { terminals, activeTerminalId, setActive, open, close, dropEnded };
}
