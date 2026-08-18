/**
 * Streaming hook behind the Agent roster pane: the specialized agents attached to one session.
 *
 * Sourced from `ConnectionService.StreamSessionAgents`, which the facilitating daemon holds open for
 * the session's life and answers in **whole snapshots** — the first frame immediately, one more per
 * `rev` change. So the fold is an assignment, not a diff: a frame replaces the roster outright, and
 * an attach made in another browser tab lands here without anyone asking for a refresh.
 *
 * Modelled on `rpc/useLiveKitRooms`, the other snapshot-then-changes feed in this app. Like it, the
 * cleanup **aborts the call** rather than flagging the loop: this feed is silent while nothing
 * changes, so a flag alone would be read only on the next event, which on an idle session never
 * comes — the iterator would stay parked forever and the call would never settle. The abort's reach
 * is client-side only (`packages/tddy-livekit-web/src/transport.ts` cancels locally without telling
 * the peer, a TODO carried there); ending the daemon's side is a separate server-side fix.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md (AC50, AC53).
 */

import { useEffect, useState } from "react";
import { ConnectError, type Client } from "@connectrpc/connect";
import { ConnectionService, type SessionAgentEntry } from "../../gen/connection_pb";

export interface SessionAgentRosterState {
  /** The roster as of the last frame. Empty until the first one arrives. */
  readonly agents: SessionAgentEntry[];
  /** Whether the first frame has arrived — what tells a genuinely empty roster from a loading one. */
  readonly hasSnapshot: boolean;
  /** Why the read failed, or `null` while it is healthy. */
  readonly error: string | null;
}

export interface SessionAgentRosterParams {
  readonly client: Client<typeof ConnectionService>;
  readonly sessionToken: string;
  readonly sessionId: string;
  /** The daemon facilitating the session; empty addresses the daemon serving this call. */
  readonly daemonInstanceId: string;
  /**
   * Whether that daemon is reachable at all. A disconnected host is not a slow one: opening a stream
   * to it would leave the pane loading forever, so the caller's own answer is respected here.
   */
  readonly enabled: boolean;
}

/** Subscribe to one session's roster for as long as the caller stays mounted. */
export function useSessionAgentRoster({
  client,
  sessionToken,
  sessionId,
  daemonInstanceId,
  enabled,
}: SessionAgentRosterParams): SessionAgentRosterState {
  const [agents, setAgents] = useState<SessionAgentEntry[]>([]);
  const [hasSnapshot, setHasSnapshot] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!enabled) return;
    const controller = new AbortController();
    // A new subscription starts over: the daemon's first frame is an authoritative snapshot.
    setAgents([]);
    setHasSnapshot(false);
    setError(null);

    (async () => {
      try {
        for await (const roster of client.streamSessionAgents(
          { sessionToken, sessionId, daemonInstanceId },
          { signal: controller.signal },
        )) {
          setAgents(roster.agents);
          setHasSnapshot(true);
        }
      } catch (err) {
        // The abort this effect's cleanup raises is the unmount/re-subscribe path; ignore it. Any
        // other failure is the daemon's own and is reported verbatim — a roster nobody could read
        // must never render as a roster with nothing in it.
        if (!controller.signal.aborted) {
          setError(ConnectError.from(err).rawMessage);
        }
      }
    })();

    return () => {
      controller.abort();
    };
  }, [client, sessionToken, sessionId, daemonInstanceId, enabled]);

  return { agents, hasSnapshot, error };
}
