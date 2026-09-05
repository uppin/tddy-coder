/**
 * A session's terminal byte stream, however it is carried.
 *
 * `GhosttyTerminalGrpc` already had the right abstraction — `GrpcStream` / `GrpcFrame` — it just had
 * one transport's name on it and one implementation. `GhosttyTerminalLiveKit` had no equivalent
 * because it built its own `TerminalService` client and connected its own `Room`; node 3 moved the
 * room, the identity and the token onto the `SessionConnection`, which is what leaves the two
 * components differing in nothing but a name.
 *
 * Promoting the interface here is most of what makes one terminal possible — and, because the
 * history path travels with it, is what gives a LiveKit-carried session scrollback it has never had.
 *
 * PRD: `docs/dev/1-WIP/2026-09-05-optional-livekit-terminal-convergence-prd.md`.
 */

/**
 * One frame of terminal output.
 *
 * `endOffset` and `atOldest` carry the replay metadata that arrives on the initial frame; live tail
 * frames leave them at their zero defaults. Both are what the forward-history loader aligns against,
 * and mis-reading them is what produced the 220-column garbling on reconnect.
 */
export interface TerminalFrame {
  readonly data: Uint8Array;
  readonly endOffset: bigint;
  readonly atOldest: boolean;
}

/** The duplex byte pipe a terminal reads and writes. */
export interface TerminalStream {
  send(data: Uint8Array): void;
  onMessage(fn: (frame: TerminalFrame) => void): void;
  close(): void;
}

/** One page of older history, as the loader consumes it. */
export interface TerminalHistoryChunk {
  readonly data: Uint8Array;
  readonly startOffset: bigint;
  readonly endOffset: bigint;
  readonly atOldest: boolean;
}

/**
 * Fetch one forward chunk of older history starting at `fromOffset`, bounded by `untilOffset` (the
 * anchor). Resolves `null` when the backend has no chunk for that range.
 */
export type TerminalHistoryFetcher = (
  fromOffset: bigint,
  untilOffset: bigint,
) => Promise<TerminalHistoryChunk | null>;

/**
 * What a session connection offers a terminal.
 *
 * `history` is optional because not every transport can serve it. A connection that offers none
 * degrades to live-tail only — which is the LiveKit path's *current* behaviour, so nothing regresses
 * while every transport that can serve history now does.
 */
export interface TerminalFeed {
  readonly stream: TerminalStream;
  readonly history?: TerminalHistoryFetcher;
}

/**
 * Whether a terminal fed by `feed` can scroll back past what is live.
 *
 * One predicate rather than `feed.history !== undefined` scattered through the component, for the
 * same reason `useHasCapability` exists: the question is asked in several places and must be
 * answered identically in all of them.
 */
export function feedSupportsHistory(feed: TerminalFeed): boolean {
  // TODO(terminal-convergence): implement
  throw new Error("feedSupportsHistory is not implemented yet");
}
