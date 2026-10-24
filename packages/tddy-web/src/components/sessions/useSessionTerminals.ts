import { useCallback, useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { TerminalSessionService } from "../../gen/terminal_session_pb";
import { tddyDebug } from "../../lib/debugMask";
import { hostNotConnectedText, rpcFailureText } from "../../rpc/rpcFailure";
import { BASH_KIND } from "./cliHarnessOptions";

const dTerm = tddyDebug("tddy:term:tabs");

type TerminalClient = Client<typeof TerminalSessionService>;

/** The reserved main terminal id — the coding Agent tab. Never handed out by StartTerminalSession. */
export const AGENT_TERMINAL_ID = "main";

/** A started terminal of this session: its id, and what it runs (`bash`, or a harness's session type). */
export interface SessionTerminal {
  readonly terminalId: string;
  readonly kind: string;
}

export interface SessionTerminalsController {
  /** Terminals for this session, in open order (the Agent/`main` terminal is implicit). */
  readonly terminals: readonly SessionTerminal[];
  /** The currently focused terminal id — `AGENT_TERMINAL_ID` or one of `terminals`. */
  readonly activeTerminalId: string;
  /** Focus an existing terminal tab. */
  setActive: (terminalId: string) => void;
  /** Start a terminal of `kind` (`bash`, or a harness's session type), append its tab, focus it. */
  open: (kind: string) => void;
  /** Stop a terminal, remove its tab, and fall back to the Agent tab if it was focused. */
  close: (terminalId: string) => void;
  /** Drop a tab whose terminal has already ended (its output stream closed) — no Stop RPC. */
  dropEnded: (terminalId: string) => void;
  /** Why the last list / open / close failed, ready to render; `null` once the next one succeeds. */
  readonly error: string | null;
}

export interface UseSessionTerminalsParams {
  sessionId: string;
  sessionToken: string;
  /** Client used for the terminal RPCs (the daemon client for gRPC sessions, the session-scoped
   *  client for LiveKit sessions). `null` until it is available; listing waits for it. */
  client: TerminalClient | null;
  /** Current terminal-control lease token, forwarded to Start/Stop when a controller is active. */
  controlToken?: string;
}

/**
 * Owns the set of terminals for a single session plus the active-tab selection.
 *
 * On attach it lists the session's already-open terminals, each with the kind it runs. `open` and
 * `close` drive the daemon `StartTerminalSession`/`StopTerminalSession` RPCs and keep the local tab
 * set consistent with an optimistic remove (the tab disappears immediately, the Stop RPC fires in
 * the background). The reserved `main` terminal (the Agent tab) is implicit and is never part of
 * `terminals`.
 *
 * A failed List/Start/Stop — or an open/close with no client to carry it — is reported as `error`
 * rather than only logged. Listing alone waits for a `null` client instead of reporting it: the
 * client is `null` while the session is still connecting, which the connection overlay already
 * shows, and the list re-runs once it arrives.
 */
export function useSessionTerminals({
  sessionId,
  sessionToken,
  client,
  controlToken = "",
}: UseSessionTerminalsParams): SessionTerminalsController {
  const [terminals, setTerminals] = useState<SessionTerminal[]>([]);
  const [activeTerminalId, setActiveTerminalId] = useState<string>(AGENT_TERMINAL_ID);
  const [error, setError] = useState<string | null>(null);

  // Populate the terminal tabs from the session's current terminals once a client is available.
  useEffect(() => {
    if (!client) return;
    let cancelled = false;
    void (async () => {
      try {
        const res = await client.listTerminalSessions({ sessionToken, sessionId });
        if (cancelled) return;
        const listed = res.terminals
          .filter((t) => t.terminalId && t.terminalId !== AGENT_TERMINAL_ID)
          .map((t) => ({ terminalId: t.terminalId, kind: t.kind }));
        setTerminals((prev) => {
          // Merge: keep any locally-opened tab not yet reflected by the daemon list.
          const merged = [...listed];
          for (const t of prev) {
            if (!merged.some((m) => m.terminalId === t.terminalId)) merged.push(t);
          }
          return merged;
        });
        setError(null);
      } catch (err) {
        dTerm(
          "listTerminalSessions failed sessionId=%s error=%o",
          sessionId,
          err instanceof Error ? err.message : err,
        );
        if (!cancelled) setError(rpcFailureText(err));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [client, sessionId, sessionToken]);

  const setActive = useCallback((id: string) => {
    setActiveTerminalId(id);
  }, []);

  const open = useCallback(
    (kind: string) => {
      if (!client) {
        setError(hostNotConnectedText(null));
        return;
      }
      // The login shell is the empty kind on the wire.
      const requestedKind = kind === BASH_KIND ? "" : kind;
      void (async () => {
        try {
          const res = await client.startTerminalSession({
            sessionToken,
            sessionId,
            controlToken,
            kind: requestedKind,
          });
          const id = res.terminalId;
          if (!id) {
            throw new Error("The host started no terminal — it answered without a terminal id.");
          }
          setTerminals((prev) =>
            prev.some((t) => t.terminalId === id) ? prev : [...prev, { terminalId: id, kind }],
          );
          setActiveTerminalId(id);
          setError(null);
        } catch (err) {
          dTerm(
            "startTerminalSession failed sessionId=%s kind=%s error=%o",
            sessionId,
            kind,
            err instanceof Error ? err.message : err,
          );
          setError(rpcFailureText(err));
        }
      })();
    },
    [client, sessionId, sessionToken, controlToken],
  );

  const dropEnded = useCallback((id: string) => {
    setTerminals((prev) => prev.filter((t) => t.terminalId !== id));
    setActiveTerminalId((prev) => (prev === id ? AGENT_TERMINAL_ID : prev));
  }, []);

  const close = useCallback(
    (id: string) => {
      // Optimistic remove — the tab and its pane come down immediately; the Stop RPC follows.
      dropEnded(id);
      if (!client) {
        setError(hostNotConnectedText(null));
        return;
      }
      client
        .stopTerminalSession({ sessionToken, sessionId, terminalId: id, controlToken })
        .then(() => setError(null))
        .catch((err: unknown) => {
          dTerm(
            "stopTerminalSession failed sessionId=%s terminalId=%s error=%o",
            sessionId,
            id,
            err instanceof Error ? err.message : err,
          );
          // The tab is already gone, so the reason names what may still be running on the host.
          setError(`${id} may still be running — stopping it failed: ${rpcFailureText(err)}`);
        });
    },
    [client, sessionId, sessionToken, controlToken, dropEnded],
  );

  return { terminals, activeTerminalId, setActive, open, close, dropEnded, error };
}
