/**
 * `SessionManager` — the single place that owns the sessions drawer's list, its refresh, and the
 * change events, so the screen component doesn't scatter that logic across effects.
 *
 * It merges two sources into one sorted, de-duplicated list:
 * - the **selected host's** sessions, pulled via an injected `fetcher` (RPC), refreshed on demand and
 *   whenever the window-bound `sessionsRefreshBridge` fires (`requestSessionsRefresh()` from anywhere);
 * - the **live cross-host** sessions, observed as LiveKit coder participants in the common room.
 *
 * It is a plain observable store (subscribe/notify, snapshot cached for `useSyncExternalStore`) with
 * no React or RPC dependency of its own — those are fed in — so it is unit-testable in isolation.
 */

import { useCallback, useEffect, useMemo, useRef, useSyncExternalStore } from "react";
import type { Client } from "@connectrpc/connect";
import { ConnectionService, type SessionEntry } from "../../gen/connection_pb";
import { sortSessionsByCreation } from "../../utils/sessionSort";
import {
  mergeActiveAndFetchedSessions,
  sessionParticipantsFromParticipants,
  type SessionParticipant,
} from "../../utils/crossHostSessions";
import { subscribeSessionsRefresh } from "../../lib/sessionsRefreshBridge";

/** Pulls the selected host's sessions (typically `client.listSessions(...)`). */
export type SessionFetcher = () => Promise<SessionEntry[]>;

export class SessionManager {
  private selectedHostSessions: SessionEntry[] = [];
  private optimistic: SessionEntry[] = [];
  private activeParticipants: SessionParticipant[] = [];
  private selectedInstanceId = "";
  private fetcher: SessionFetcher | null = null;
  private cached: SessionEntry[] = [];
  private readonly listeners = new Set<() => void>();
  private unsubscribeBridge: (() => void) | null = null;

  /** Start listening for window-bound refresh requests. Returns a disposer. */
  start(): () => void {
    this.unsubscribeBridge ??= subscribeSessionsRefresh(() => this.refresh());
    return () => this.stop();
  }

  stop(): void {
    this.unsubscribeBridge?.();
    this.unsubscribeBridge = null;
  }

  /** Set the selected host's session source; triggers an immediate refresh. */
  setFetcher(fetcher: SessionFetcher | null): void {
    this.fetcher = fetcher;
    this.refresh();
  }

  setActiveParticipants(participants: SessionParticipant[]): void {
    this.activeParticipants = participants;
    this.recompute();
  }

  setSelectedInstanceId(instanceId: string): void {
    if (instanceId === this.selectedInstanceId) return;
    this.selectedInstanceId = instanceId;
    this.recompute();
  }

  /** Optimistically add a session (e.g. a background pr-stack start) until the next refresh. */
  addOptimisticSession(entry: SessionEntry): void {
    if (this.optimistic.some((s) => s.sessionId === entry.sessionId)) return;
    this.optimistic = [...this.optimistic, entry];
    this.recompute();
  }

  /** Re-pull the selected host's sessions. No-op until a fetcher is set. */
  refresh(): void {
    const fetcher = this.fetcher;
    if (!fetcher) return;
    fetcher()
      .then((sessions) => {
        this.selectedHostSessions = sessions;
        this.recompute();
      })
      .catch((err) => console.debug("[SessionManager] refresh error", err));
  }

  /** The current merged, sorted list. Stable reference until the next change (safe for snapshots). */
  get sessions(): SessionEntry[] {
    return this.cached;
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private recompute(): void {
    const byId = new Map<string, SessionEntry>();
    for (const s of this.selectedHostSessions) byId.set(s.sessionId, s);
    for (const s of this.optimistic) if (!byId.has(s.sessionId)) byId.set(s.sessionId, s);
    this.cached = sortSessionsByCreation(
      mergeActiveAndFetchedSessions(
        Array.from(byId.values()),
        this.activeParticipants,
        this.selectedInstanceId,
      ),
    );
    for (const listener of this.listeners) listener();
  }
}

/**
 * React binding for a per-screen {@link SessionManager}: wires the RPC client, common-room
 * participants, and selected host into the manager, and returns its reactive merged list plus a way
 * to add optimistic entries.
 */
export function useSessionManager(
  client: Client<typeof ConnectionService> | null,
  sessionToken: string,
  participants: ReadonlyArray<{ identity: string }>,
  selectedInstanceId: string,
): { sessions: SessionEntry[]; addOptimisticSession: (entry: SessionEntry) => void } {
  const managerRef = useRef<SessionManager | null>(null);
  managerRef.current ??= new SessionManager();
  const manager = managerRef.current;

  useEffect(() => manager.start(), [manager]);

  useEffect(() => {
    manager.setFetcher(
      client
        ? () => client.listSessions({ sessionToken }).then((resp) => resp.sessions as SessionEntry[])
        : null,
    );
  }, [manager, client, sessionToken]);

  const activeParticipants = useMemo(
    () => sessionParticipantsFromParticipants(participants),
    [participants],
  );
  useEffect(() => {
    manager.setActiveParticipants(activeParticipants);
  }, [manager, activeParticipants]);

  useEffect(() => {
    manager.setSelectedInstanceId(selectedInstanceId);
  }, [manager, selectedInstanceId]);

  const sessions = useSyncExternalStore(
    (listener) => manager.subscribe(listener),
    () => manager.sessions,
    () => manager.sessions,
  );

  const addOptimisticSession = useCallback(
    (entry: SessionEntry) => manager.addOptimisticSession(entry),
    [manager],
  );

  return { sessions, addOptimisticSession };
}
