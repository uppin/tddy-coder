/**
 * `SessionRuntimeRegistry` — the per-session runtime store that keeps one mounted terminal per
 * attached session and survives focus switches. A backgrounded session's runtime stays alive
 * (receiving bytes) until it is explicitly disconnected; there is no auto-eviction cap.
 *
 * Observable store (subscribe/notify, snapshot cached for `useSyncExternalStore`); no React or RPC
 * dependency of its own — those are fed in by the screen — so it is unit-testable in isolation.
 *
 * The connection fields (`status`, `livekitUrl`, `livekitRoom`, `livekitServerIdentity`,
 * `identity`, `room`) are optional so the store remains usable in isolation tests that only
 * exercise focus/eviction/byte accounting. The screen populates them from the attach response and
 * the terminal's `onRoom` callback; the runtime layer renders each attached session's terminal
 * from them.
 *
 * Changeset: `2026-07-12-fast-session-change`
 * Feature: `docs/ft/web/session-drawer.md#fast-session-change` (req 2, 3)
 */

import type { Room } from "livekit-client";

export type SessionRuntimeStatus = "connected-livekit" | "connected-grpc";

export interface SessionRuntimeState {
  readonly sessionId: string;
  /** True while the session's LiveKit room + terminal are attached (mounted). */
  attached: boolean;
  /** Attachment status — drives which terminal component the runtime layer renders. */
  status?: SessionRuntimeStatus;
  /** LiveKit server URL for the session's terminal room (`connected-livekit` only). */
  livekitUrl?: string;
  /** LiveKit room name for the session's terminal room (`connected-livekit` only). */
  livekitRoom?: string;
  /** The coder participant identity the session's terminal room is routed to. */
  livekitServerIdentity?: string;
  /** This browser tab's own LiveKit identity for joining the session's terminal room. */
  identity?: string;
  /** The session's connected LiveKit `Room`, captured via the terminal's `onRoom` callback.
   *  Production uses it to build the session-scoped `ConnectionService` client; `null` in tests
   *  (the test-double `liveKitFactory` ignores its `room` argument). */
  room?: Room | null;
  /** Cumulative bytes received over the session's LiveKit transport. */
  bytesIn: number;
  /** Cumulative bytes sent over the session's LiveKit transport. */
  bytesOut: number;
  /** Epoch-ms of the most recent `DataReceived` event, or `null` when none has arrived yet. */
  lastDataReceivedAt: number | null;
}

/** Connection-only patch applied when an attach response arrives for an existing runtime. */
export interface SessionRuntimeConnection {
  status: SessionRuntimeStatus;
  livekitUrl: string;
  livekitRoom: string;
  livekitServerIdentity: string;
  identity: string;
}

export interface ByteSample {
  bytesIn: number;
  bytesOut: number;
  /** Epoch-ms when the sample was observed. */
  at: number;
}

/** A single terminal I/O event, as fired by the terminal's `onBytes` sink. One direction per event:
 *  an output chunk carries `bytesIn`, a batched input yield carries `bytesOut`. Either field omitted
 *  ⇒ 0. */
export interface ByteDelta {
  bytesIn?: number;
  bytesOut?: number;
}

/**
 * Bind an `onBytes` sink to one session's runtime. Each call folds the delta into the registry's
 * cumulative counters and stamps `lastDataReceivedAt` from `now()` (default {@link Date.now}). This
 * is the bridge the terminal fires per output chunk / input yield so a backgrounded session's byte
 * counters keep ticking in the inspector.
 */
export function makeByteTap(
  registry: SessionRuntimeRegistry,
  sessionId: string,
  now: () => number = () => Date.now(),
): (delta: ByteDelta) => void {
  return (delta: ByteDelta) => {
    registry.recordBytes(sessionId, {
      bytesIn: delta.bytesIn ?? 0,
      bytesOut: delta.bytesOut ?? 0,
      at: now(),
    });
  };
}

export class SessionRuntimeRegistry {
  private readonly runtimeBySessionId = new Map<string, SessionRuntimeState>();
  private focusedId: string | null = null;
  private readonly listeners = new Set<() => void>();
  /** Cached snapshot of `runtimes` for `useSyncExternalStore` (rebuilt only on `notify`). */
  private cachedRuntimes: SessionRuntimeState[] = [];

  /** Add (or replace) a session's runtime state. Does not change focus. */
  add(sessionId: string, state: SessionRuntimeState): void {
    this.runtimeBySessionId.set(sessionId, state);
    if (this.focusedId === null) this.focusedId = sessionId;
    this.notify();
  }

  /** Patch an existing runtime's connection params (from an attach response) without resetting
   *  byte counters. No-op when the session has no runtime. */
  updateConnection(sessionId: string, conn: SessionRuntimeConnection): void {
    const state = this.runtimeBySessionId.get(sessionId);
    if (!state) return;
    this.runtimeBySessionId.set(sessionId, {
      ...state,
      status: conn.status,
      livekitUrl: conn.livekitUrl,
      livekitRoom: conn.livekitRoom,
      livekitServerIdentity: conn.livekitServerIdentity,
      identity: conn.identity,
    });
    this.notify();
  }

  /** Store the session's connected LiveKit `Room` (captured from the terminal). No-op when the
   *  session has no runtime. */
  setRoom(sessionId: string, room: Room): void {
    const state = this.runtimeBySessionId.get(sessionId);
    if (!state) return;
    this.runtimeBySessionId.set(sessionId, { ...state, room });
    // `room` is not part of the `runtimes` snapshot used by the UI; no notify needed.
  }

  /** Focus a session's runtime. The previously focused runtime stays mounted (backgrounded). */
  focus(sessionId: string): void {
    if (!this.runtimeBySessionId.has(sessionId)) return;
    this.focusedId = sessionId;
    this.notify();
  }

  /** Explicitly disconnect (evict) a session's runtime. Other runtimes are unaffected. */
  disconnect(sessionId: string): void {
    this.runtimeBySessionId.delete(sessionId);
    if (this.focusedId === sessionId) this.focusedId = null;
    this.notify();
  }

  /** The focused session id, or `null` when no runtime is focused. */
  get focusedSessionId(): string | null {
    return this.focusedId;
  }

  /** A runtime state by session id, or `undefined` when not mounted. */
  get(sessionId: string): SessionRuntimeState | undefined {
    return this.runtimeBySessionId.get(sessionId);
  }

  /** All mounted runtimes (focused first, then the rest in insertion order). */
  get runtimes(): SessionRuntimeState[] {
    return this.cachedRuntimes;
  }

  private rebuildRuntimes(): void {
    const focused = this.focusedId;
    const entries = Array.from(this.runtimeBySessionId.values());
    if (focused && this.runtimeBySessionId.has(focused)) {
      const head = this.runtimeBySessionId.get(focused)!;
      this.cachedRuntimes = [head, ...entries.filter((r) => r.sessionId !== focused)];
    } else {
      this.cachedRuntimes = entries;
    }
  }

  /** Update a (background or focused) runtime's byte counters. `lastDataReceivedAt` tracks inbound
   *  data only, so it is stamped from `sample.at` solely when the sample carries received bytes. */
  recordBytes(sessionId: string, sample: ByteSample): void {
    const state = this.runtimeBySessionId.get(sessionId);
    if (!state) return;
    state.bytesIn += sample.bytesIn;
    state.bytesOut += sample.bytesOut;
    // "last data received" tracks inbound only — a pure-outbound event (user input) must not reset it.
    if (sample.bytesIn > 0) state.lastDataReceivedAt = sample.at;
    this.notify();
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  private notify(): void {
    this.rebuildRuntimes();
    for (const listener of this.listeners) listener();
  }
}
