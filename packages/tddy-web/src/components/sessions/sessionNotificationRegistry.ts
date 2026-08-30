/**
 * `SessionNotificationRegistry` — the per-session, module-level store behind the drawer's
 * indicators. It holds three moments per session: when it last reported activity, when it last
 * asked for attention, and when the operator last looked at it.
 *
 * It mirrors {@link AgentActivityRegistry}: an observable store (subscribe/notify with an
 * immutably-replaced per-session snapshot) consumed via `useSyncExternalStore`. Because a session's
 * state object is replaced only when it actually changes, `get(sessionId)` returns a reference
 * stable across notifications that don't touch that session — the contract `useSyncExternalStore`
 * requires, and the reason a write for one row does not tear every other row's snapshot.
 *
 * The store holds moments only; the dot itself is derived from them by `sessionIndicatorFor`
 * (`lib/sessionIndicator.ts`), so the decay of a blink needs no timer in here. It is in-memory only
 * and does not survive a full page reload — a reloaded dashboard starts with every row settled,
 * which is the honest state: nothing has been streamed to it yet.
 *
 * PRD: docs/ft/daemon/session-notifications.md (FR4–FR6).
 */

import type { SessionNotificationState } from "../../lib/sessionIndicator";

export type { SessionNotificationState };

/** A session the stream has mentioned but which has recorded nothing yet. */
function freshState(): SessionNotificationState {
  return { lastActivityAtMs: 0, attentionAtMs: 0, seenAtMs: 0 };
}

/** Cap on retained per-session state, matching {@link AgentActivityRegistry}. A long-lived
 *  dashboard can see many sessions; beyond this the least-recently-written entries are evicted. An
 *  evicted session simply renders as steady green until its next notification, which is what a
 *  session nobody has heard from in a hundred other sessions' worth of traffic looks like anyway. */
const MAX_SESSIONS = 100;

export class SessionNotificationRegistry {
  private readonly bySessionId = new Map<string, SessionNotificationState>();
  private readonly listeners = new Set<() => void>();

  /** The per-session state, or `undefined` when the session has never been heard of. */
  get(sessionId: string): SessionNotificationState | undefined {
    return this.bySessionId.get(sessionId);
  }

  /** Record that the session reported activity at `atMs`. Newest wins: a reconnect can replay an
   *  event behind one already applied, and an older moment must not walk the dot backwards. */
  recordActivity(sessionId: string, atMs: number): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState();
    if (atMs <= prev.lastActivityAtMs) return;
    this.write(sessionId, { ...prev, lastActivityAtMs: atMs });
  }

  /** Record that the session asked for attention at `atMs`. Newest wins, as for activity. */
  recordAttention(sessionId: string, atMs: number): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState();
    if (atMs <= prev.attentionAtMs) return;
    this.write(sessionId, { ...prev, attentionAtMs: atMs });
  }

  /** Record that the operator viewed the session at `atMs` — the baseline everything outstanding is
   *  measured against. Works for a session the stream has never mentioned: opening a quiet session
   *  still establishes when it was last looked at, so notifications arriving after are outstanding
   *  and ones arriving before are not. */
  markSeen(sessionId: string, atMs: number): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState();
    if (atMs <= prev.seenAtMs) return;
    this.write(sessionId, { ...prev, seenAtMs: atMs });
  }

  /** Store `next` as the session's state (most-recently-written), evict the oldest entries beyond
   *  the cap, then notify. Re-inserting moves the key to the end of the Map so eviction is LRU.
   *  Every caller checks first that the write changes something: a needless notify re-renders every
   *  row in the drawer, and the stream carries a frame per session per turn. */
  private write(sessionId: string, next: SessionNotificationState): void {
    this.bySessionId.delete(sessionId);
    this.bySessionId.set(sessionId, next);
    while (this.bySessionId.size > MAX_SESSIONS) {
      const oldest = this.bySessionId.keys().next().value;
      if (oldest === undefined) break;
      this.bySessionId.delete(oldest);
    }
    this.notify();
  }

  /** Drop every session's notifications. Used to isolate component tests that reuse a `sessionId`
   *  across cases; in production the store lives for the app's lifetime. */
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

/** App-lifetime singleton — the notification stream writes to it and every drawer row reads from
 *  it, so one subscription serves the whole drawer (NFR1). */
export const sessionNotificationRegistry = new SessionNotificationRegistry();
