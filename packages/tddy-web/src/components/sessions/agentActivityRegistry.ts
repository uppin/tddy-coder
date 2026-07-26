/**
 * `AgentActivityRegistry` — the per-session, module-level store that backs the Agent Activity
 * overlay. It holds the streamed activity `count`, the lazily-pulled transcript `messages`, the
 * `seenCount` baseline (for the unread badge), and whether the heavy snapshot has already
 * `snapshotLoaded`, keyed by `sessionId`.
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
  /** True once the `SNAPSHOT_THEN_LIVE` transcript pull has **delivered** for this session. A pull
   *  that started but was cancelled before its first frame landed leaves this false, so it is
   *  retried on the next open rather than caching an empty transcript forever. */
  readonly snapshotLoaded: boolean;
}

/** The lazily-fetched body of one tool call, as shown by the detail dialog. `loading` while the
 *  `GetAcpToolCallDetail` lookup is in flight, `loaded` once it resolves (either or both bodies may
 *  be absent for a call with no recorded input/output), `error` if the lookup fails. An `error` is
 *  not a permanent cache entry — re-opening the same row retries (see {@link useToolCallDetail}). */
export type ToolCallBodyState =
  | { readonly status: "loading" }
  | { readonly status: "loaded"; readonly rawInput?: string; readonly rawOutput?: string }
  | { readonly status: "error"; readonly error: string };

const EMPTY_MESSAGES: ChatMessage[] = [];

function freshState(sessionId: string): AgentActivityState {
  return { sessionId, count: 0, messages: EMPTY_MESSAGES, seenCount: 0, snapshotLoaded: false };
}

/** Cap on retained per-session state. A long-lived dashboard can visit many sessions; beyond this
 *  the least-recently-written entries are evicted (a revisited session simply re-pulls its snapshot,
 *  which the lazy design already supports). */
const MAX_SESSIONS = 100;

export class AgentActivityRegistry {
  private readonly bySessionId = new Map<string, AgentActivityState>();
  /** Lazily-fetched tool-call bodies, nested `sessionId → (callId → body)`. Held separately from the
   *  per-session transcript state so caching a body never replaces `AgentActivityState` — the
   *  transcript reference stays stable across body writes (the contract `useSyncExternalStore`
   *  requires for the transcript subscription). */
  private readonly bodiesBySession = new Map<string, Map<string, ToolCallBodyState>>();
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

  /** Mark that the snapshot pull has delivered (its first frame landed), so a later switch-back
   *  reuses the cache instead of re-opening the stream. No-op (no notify) when already loaded. */
  markSnapshotLoaded(sessionId: string): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState(sessionId);
    if (prev.snapshotLoaded) return;
    this.write(sessionId, { ...prev, snapshotLoaded: true });
  }

  /** Set the unread baseline to the current count (called on overlay open). No-op when unchanged. */
  markSeen(sessionId: string): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState(sessionId);
    if (prev.seenCount === prev.count) return;
    this.write(sessionId, { ...prev, seenCount: prev.count });
  }

  /** The cached body for one tool call, or `undefined` when it has never been fetched. Returns the
   *  stored object reference, so a subscriber reading the same `(sessionId, callId)` across unrelated
   *  notifications sees a stable reference (safe for `useSyncExternalStore`). */
  getBody(sessionId: string, callId: string): ToolCallBodyState | undefined {
    return this.bodiesBySession.get(sessionId)?.get(callId);
  }

  /** Cache the fetched (or in-flight, or failed) body for one tool call, then notify. Writing a body
   *  never touches the per-session transcript state, so the transcript subscription does not churn. */
  setBody(sessionId: string, callId: string, body: ToolCallBodyState): void {
    let bodies = this.bodiesBySession.get(sessionId);
    if (!bodies) {
      bodies = new Map<string, ToolCallBodyState>();
      this.bodiesBySession.set(sessionId, bodies);
    }
    bodies.set(callId, body);
    this.notify();
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
    if (this.bySessionId.size === 0 && this.bodiesBySession.size === 0) return;
    this.bySessionId.clear();
    this.bodiesBySession.clear();
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
