import { useCallback, useEffect, useState } from "react";
import type { TmuxSessionInfo } from "tddy-client-web/gen/terminal_session_pb";
import { tddyDebug } from "tddy-client-web";
import { hostNotConnectedText, rpcFailureText } from "../../rpc/rpcFailure";
import type { TerminalClient } from "./sessionRuntimeProps";

const dTmux = tddyDebug("tddy:term:tmux");

/**
 * The tmux sessions on a session's host that a "tmux" tab can attach to, fetched through the
 * owning daemon's `TerminalSessionService.ListTmuxSessions` each time the "+" menu opens — tmux
 * sessions come and go outside tddy, so a list cached from an earlier opening would offer stale
 * ones.
 *
 * Feature: docs/ft/web/session-terminal-tabs.md (`#session-tabs 3/7`)
 */
export interface TmuxMenuState {
  /** The sessions from the last listing, in the host's order. */
  readonly sessions: readonly TmuxSessionInfo[];
  /** Why the last listing failed (e.g. tmux is not installed), ready to render; `null` otherwise. */
  readonly error: string | null;
  /** True from a `refresh` until its listing answers. Never true without a client: that is the
   *  host-not-connected `error` instead, and the listing still runs when the client arrives. */
  readonly loading: boolean;
  /** List again — called when the "+" menu opens. */
  readonly refresh: () => void;
}

export interface UseTmuxSessionsParams {
  sessionId: string;
  sessionToken: string;
  /** The owning daemon's terminal client; no listing is made without one. */
  client?: TerminalClient | null;
}

/**
 * Lists the tmux sessions when asked. `refresh` records a request; the listing runs for the newest
 * request once a client is present, so a menu opened while the session is still connecting lists as
 * soon as the client arrives, the same way the terminal list waits for its client. A listing that
 * a newer request has overtaken, or that outlives the component, is not written back.
 */
export function useTmuxSessions({
  sessionId,
  sessionToken,
  client = null,
}: UseTmuxSessionsParams): TmuxMenuState {
  const [sessions, setSessions] = useState<readonly TmuxSessionInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  // `requests` counts the refreshes asked for; `answered` is the last one whose listing came back.
  const [requests, setRequests] = useState(0);
  const [answered, setAnswered] = useState(0);

  useEffect(() => {
    // Nothing is listed until the menu asks for it, and nothing without a client to ask through.
    if (requests === 0 || client === null) return;
    let current = true;
    void (async () => {
      try {
        const res = await client.listTmuxSessions({ sessionToken, sessionId });
        if (!current) return;
        setSessions(res.sessions);
        setError(null);
      } catch (err) {
        dTmux(
          "listTmuxSessions failed sessionId=%s error=%o",
          sessionId,
          err instanceof Error ? err.message : err,
        );
        if (!current) return;
        setError(rpcFailureText(err));
      }
      if (current) setAnswered(requests);
    })();
    return () => {
      current = false;
    };
  }, [requests, client, sessionId, sessionToken]);

  const refresh = useCallback(() => setRequests((count) => count + 1), []);

  // A listing asked for with no client has nothing to wait on but the host connecting: say so, as
  // the terminal tabs do, rather than loading for ever. When the client arrives the listing runs
  // and this gives way to its answer.
  const waitingForHost = requests > 0 && client === null;
  return {
    sessions,
    error: waitingForHost ? hostNotConnectedText(null) : error,
    loading: !waitingForHost && requests !== answered,
    refresh,
  };
}
