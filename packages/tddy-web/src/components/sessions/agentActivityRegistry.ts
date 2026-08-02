/**
 * `AgentActivityRegistry` — the per-session, module-level store that backs the Agent Activity
 * overlay. It holds the streamed activity `count`, the lazily-pulled transcript `messages`, the
 * `seenCount` baseline (for the unread badge), whether the heavy snapshot has already
 * `snapshotLoaded`, and the tool bodies fetched on demand by the detail dialog (`toolDetails`), keyed
 * by `sessionId`.
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

/** One tool call's bodies, as `ConnectionService.GetAcpToolCallDetail` resolved them. Either side may
 *  be absent: a still-running call has an input but no output yet. */
export interface ToolCallDetail {
  readonly rawInput?: string;
  readonly rawOutput?: string;
}

export interface AgentActivityState {
  readonly sessionId: string;
  /** Current number of coalesced activity entries (agent-text frames + distinct tool calls),
   *  streamed by the `COUNT_THEN_LIVE` feed. */
  readonly count: number;
  /** True once the count feed has delivered a frame for this session — **whatever** that frame said.
   *  `count` alone cannot distinguish "no activity" from "the feed has not answered yet", and a
   *  surface that renders the difference (the Activities view's empty state) must not state the
   *  former while the latter is still true. A feed that errors never sets this. */
  readonly countLoaded: boolean;
  /** The transcript entries currently loaded, oldest-first: the pages paged in behind the range
   *  followed by the range the transcript feed owns. Empty until the feed is opened. */
  readonly messages: ChatMessage[];
  /** The entries paged in from **before** where the transcript feed's range starts, oldest-first.
   *  Held apart from that range because the feed rewrites its own entries wholesale on every frame
   *  it delivers, which would otherwise drop everything paged in behind them. */
  readonly olderMessages: ChatMessage[];
  /** Absolute 0-based transcript position of the loaded range's oldest entry — the reverse cursor
   *  the next `GetAcpReplayPage` pages back from. `null` until the feed's first frame states it. */
  readonly oldestSeq: number | null;
  /** True once the loaded range reaches the transcript head: nothing older exists, so no scroll
   *  issues another page fetch. */
  readonly atOldest: boolean;
  /** True while a `GetAcpReplayPage` for this session is in flight. */
  readonly loadingOlder: boolean;
  /** The `count` value at the last overlay open — entries beyond it are unread. */
  readonly seenCount: number;
  /** True once the `SNAPSHOT_THEN_LIVE` transcript pull has **delivered** for this session. A pull
   *  that started but was cancelled before its first frame landed leaves this false, so it is
   *  retried on the next open rather than caching an empty transcript forever. */
  readonly snapshotLoaded: boolean;
  /** Tool-call bodies already fetched for this session, keyed by `tool_call_id`. Populated by the
   *  detail dialog's lookup so reopening a row costs no second request. Only settled calls are
   *  cached — a still-running call's output can still arrive, so caching its partial body would keep
   *  it stale for the rest of the session. */
  readonly toolDetails: ReadonlyMap<string, ToolCallDetail>;
}

const EMPTY_MESSAGES: ChatMessage[] = [];
const EMPTY_TOOL_DETAILS: ReadonlyMap<string, ToolCallDetail> = new Map();

function freshState(sessionId: string): AgentActivityState {
  return {
    sessionId,
    count: 0,
    countLoaded: false,
    messages: EMPTY_MESSAGES,
    olderMessages: EMPTY_MESSAGES,
    oldestSeq: null,
    atOldest: false,
    loadingOlder: false,
    seenCount: 0,
    snapshotLoaded: false,
    toolDetails: EMPTY_TOOL_DETAILS,
  };
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

  /** Record the latest streamed activity count, and mark the count feed as having answered. No-op
   *  (no notify) only when both are already what this frame reports — the first frame of a session
   *  with zero entries still flips `countLoaded`, which is the whole point of tracking it. */
  setCount(sessionId: string, count: number): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState(sessionId);
    if (prev.count === count && prev.countLoaded) return;
    this.write(sessionId, { ...prev, count, countLoaded: true });
  }

  /** Replace the entries the transcript feed owns — the range it opened on plus whatever it has
   *  tailed since. Pages already prepended behind that range are kept ahead of them, so a live frame
   *  (which re-states the whole fed range) cannot drop history the operator paged in. */
  setMessages(sessionId: string, messages: ChatMessage[]): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState(sessionId);
    const combined =
      prev.olderMessages.length === 0 ? messages : [...prev.olderMessages, ...messages];
    this.write(sessionId, { ...prev, messages: combined });
  }

  /** Record where the loaded range starts, as the transcript feed's first frame states it, and
   *  whether that already reaches the transcript head. No-op (no notify) when unchanged. */
  setOldestSeq(sessionId: string, seq: number, atOldest: boolean): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState(sessionId);
    if (prev.oldestSeq === seq && prev.atOldest === atOldest) return;
    this.write(sessionId, { ...prev, oldestSeq: seq, atOldest });
  }

  /** Flag a `GetAcpReplayPage` as in flight (or no longer so). A **failed** fetch clears it through
   *  this same writer and leaves the loaded range untouched, so a later scroll retries. No-op (no
   *  notify) when unchanged. */
  setLoadingOlder(sessionId: string, loadingOlder: boolean): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState(sessionId);
    if (prev.loadingOlder === loadingOlder) return;
    this.write(sessionId, { ...prev, loadingOlder });
  }

  /**
   * Prepend a resolved older page above the loaded range and move the reverse cursor to its start.
   *
   * A page whose `firstSeq` is not strictly below the current cursor is **ignored**: it is either a
   * duplicate of one already prepended or an answer to a cursor that has since moved, and rendering
   * it would double the same entries. That arithmetic is the only thing standing between an
   * in-flight response arriving twice and a doubled transcript, which is why the cursor is an
   * absolute position rather than an opaque token. A page arriving before the feed has stated where
   * the range starts is ignored for the same reason — there is nothing to compare it against.
   */
  prependMessages(
    sessionId: string,
    messages: ChatMessage[],
    firstSeq: number,
    atOldest: boolean,
  ): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState(sessionId);
    if (prev.oldestSeq === null || firstSeq >= prev.oldestSeq) return;
    const olderMessages = [...messages, ...prev.olderMessages];
    const fedRange = prev.messages.slice(prev.olderMessages.length);
    this.write(sessionId, {
      ...prev,
      olderMessages,
      messages: [...olderMessages, ...fedRange],
      oldestSeq: firstSeq,
      atOldest,
    });
  }

  /** Mark that the snapshot pull has delivered (its first frame landed), so a later switch-back
   *  reuses the cache instead of re-opening the stream. No-op (no notify) when already loaded. */
  markSnapshotLoaded(sessionId: string): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState(sessionId);
    if (prev.snapshotLoaded) return;
    this.write(sessionId, { ...prev, snapshotLoaded: true });
  }

  /** Cache one tool call's fetched bodies under its `tool_call_id`. The map is replaced (never
   *  mutated) so the session's state object stays a snapshot, keeping `get()` reference-stable for
   *  `useSyncExternalStore`. Callers cache settled calls only — see
   *  {@link AgentActivityState.toolDetails}. No-op (no notify) when the same bodies are already
   *  cached, so a repeated fetch of an unchanged call does not re-render the transcript.
   *
   *  Bodies live **in** the session snapshot rather than in a side map, which does mean the
   *  transcript's `useSyncExternalStore` subscriber re-renders once when a body is first cached
   *  (once per opened row). That is accepted deliberately: `messages` keeps its own array identity
   *  across the write, so the rendered rows reconcile without work, and one store beats two that can
   *  disagree about which sessions exist or when to evict them. */
  setToolDetail(sessionId: string, toolCallId: string, detail: ToolCallDetail): void {
    const prev = this.bySessionId.get(sessionId) ?? freshState(sessionId);
    const cached = prev.toolDetails.get(toolCallId);
    if (cached && cached.rawInput === detail.rawInput && cached.rawOutput === detail.rawOutput) {
      return;
    }
    const toolDetails = new Map(prev.toolDetails);
    toolDetails.set(toolCallId, detail);
    this.write(sessionId, { ...prev, toolDetails });
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
