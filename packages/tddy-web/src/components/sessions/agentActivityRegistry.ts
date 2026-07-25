/**
 * `AgentActivityRegistry` — the per-session, module-level store that backs the Agent Activity
 * overlay. It holds the streamed activity `count`, the lazily-pulled transcript `messages`, the
 * `seenCount` baseline (for the unread badge), and whether the heavy snapshot has already been
 * `snapshotOpened`, keyed by `sessionId`.
 *
 * It mirrors {@link SessionRuntimeRegistry}: an observable store (subscribe/notify with an
 * immutably-replaced per-session snapshot) consumed via `useSyncExternalStore`. Because a session's
 * state object is replaced only when it actually changes, `get(sessionId)` returns a reference stable
 * across unrelated notifications — the contract `useSyncExternalStore` requires.
 *
 * The store is a singleton for the app's lifetime, so switching to another session and back reuses
 * the cached transcript + count instead of re-pulling the snapshot. It is in-memory only and does not
 * survive a full page reload.
 *
 * PRD: docs/ft/web/agent-activity-pane.md § Persisted, lazily-counted activity (§1 persistence).
 */

import type { ChatMessage } from "../chat/useAgentChat";

export interface AgentActivityState {
  readonly sessionId: string;
  /** Current number of coalesced activity entries (agent-text frames + distinct tool calls),
   *  streamed by the `COUNT_THEN_LIVE` feed. */
  readonly count: number;
  /** The lazily-pulled transcript entries (empty until the snapshot is opened). */
  readonly messages: ChatMessage[];
  /** The `count` value at the last overlay open — entries beyond it are unread. */
  readonly seenCount: number;
  /** True once the `SNAPSHOT_THEN_LIVE` transcript pull has been started for this session. */
  readonly snapshotOpened: boolean;
}

const EMPTY_MESSAGES: ChatMessage[] = [];

function freshState(sessionId: string): AgentActivityState {
  return { sessionId, count: 0, messages: EMPTY_MESSAGES, seenCount: 0, snapshotOpened: false };
}

/** Cap on retained per-session state. A long-lived dashboard can visit many sessions; beyond this
 *  the least-recently-written entries are evicted (a revisited session simply re-pulls its snapshot,
 *  which the lazy design already supports). */
const MAX_SESSIONS = 100;

export class AgentActivityRegistry {
  private readonly bySessionId = new Map<string, AgentActivityState>();
  private readonly listeners = new Set<() => void>();

  /** The per-session state, or `undefined` when the session has never been seen. Reference-stable
   *  across notifications that don't touch this session (safe for `useSyncExternalStore`). */
  get(sessionId: string): AgentActivityState | undefined {
    return this.bySessionId.get(sessionId);
  }

  /** Record the latest streamed activity count. No-op (no notify) when unchanged. */
  setCount(sessionId: string, count: number): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState(sessionId);
    if (prev.count === count) return;
    this.write(sessionId, { ...prev, count });
  }

  /** Replace the cached transcript entries (the resolved snapshot). */
  setMessages(sessionId: string, messages: ChatMessage[]): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState(sessionId);
    this.write(sessionId, { ...prev, messages });
  }

  /** Mark that the snapshot pull has been started, so a later switch-back reuses the cache instead
   *  of re-opening the stream. No-op (no notify) when already opened. */
  markSnapshotOpened(sessionId: string): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState(sessionId);
    if (prev.snapshotOpened) return;
    this.write(sessionId, { ...prev, snapshotOpened: true });
  }

  /** Set the unread baseline to the current count (called on overlay open). No-op when unchanged. */
  markSeen(sessionId: string): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState(sessionId);
    if (prev.seenCount === prev.count) return;
    this.write(sessionId, { ...prev, seenCount: prev.count });
  }

  /** Store `next` as the session's state (most-recently-written), evict the oldest entries beyond
   *  the cap, then notify. Re-inserting moves the key to the end of the Map so eviction is LRU. */
  private write(sessionId: string, next: AgentActivityState): void {
    this.bySessionId.delete(sessionId);
    this.bySessionId.set(sessionId, next);
    while (this.bySessionId.size > MAX_SESSIONS) {
      const oldest = this.bySessionId.keys().next().value;
      if (oldest === undefined) break;
      this.bySessionId.delete(oldest);
    }
    this.notify();
  }

  /** Drop all cached per-session state. Used to isolate component tests that reuse a `sessionId`
   *  across cases (in production a `sessionId` maps to one stable transcript, so no reset occurs). */
  reset(): void {
    if (this.bySessionId.size === 0) return;
    this.bySessionId.clear();
    this.notify();
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private notify(): void {
    for (const listener of this.listeners) listener();
  }
}

/** App-lifetime singleton — the overlay and `useAcpReplay` share this one store so per-session
 *  activity survives a session switch. */
export const agentActivityRegistry = new AgentActivityRegistry();
