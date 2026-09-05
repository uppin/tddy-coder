/**
 * A session's terminal byte stream, however it is carried.
 *
 * The gRPC terminal already had the right abstraction — `GrpcStream` / `GrpcFrame` — it just had
 * one transport's name on it and one implementation. The LiveKit terminal had no equivalent because
 * it built its own `TerminalService` client and connected its own `Room`; node 3 moved the room,
 * the identity and the token onto the `SessionConnection`, which is what left the two components
 * differing in nothing but a name.
 *
 * Promoting the interface here is most of what makes one terminal possible — and, because the
 * history path travels with it, is what gives a LiveKit-carried session scrollback it has never had.
 *
 * Docs: `packages/tddy-web/docs/terminal-session.md`.
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

  /** True when this chunk reaches the oldest retained byte — nothing exists *below* it. */
  readonly atOldest: boolean;

  /**
   * True when this chunk reaches `untilOffset` (or the capture tip) — nothing exists *above* it,
   * so the forward fill is complete.
   *
   * Not a duplicate of {@link atOldest}, and the two are read at opposite ends of the fill: the
   * first chunk of a fill from offset 0 is normally `atOldest` and not `atEnd`. This is the field
   * `TerminalHistoryForwardLoader` terminates on, and it is the *only* terminator available to a
   * fill with no offset anchor — a LiveKit-carried session's, whose frames carry no offsets at all.
   */
  readonly atEnd: boolean;
}

/**
 * Fetch one forward chunk of older history starting at `fromOffset`, bounded by `untilOffset` (the
 * anchor, or `0n` for "until the capture tip"). Resolves `null` when the backend has no chunk for
 * that range.
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

  /**
   * Settles when the remote end of this feed is gone for good — the session's process exited, or
   * the participant serving its PTY left the room.
   *
   * The terminal cannot see this for itself once the transport moved onto the connection: the loop
   * that reads the wire is the feed's, so the end of that loop is a fact only the feed holds. Both
   * predecessors acted on it — the LiveKit terminal covered the pane with "Session ended" and fired
   * `onRemoteSessionEnded`, and the daemon path evicted the runtime — so a converged terminal that
   * could not be told would leave a dead session looking interactive.
   *
   * Optional, and never rejected: a feed that has no way of knowing simply never settles it, and a
   * terminal fed by one keeps tailing. That is the honest reading of "this wire cannot tell you",
   * as against inventing an end that did not happen.
   */
  readonly ended?: Promise<void>;
}

/**
 * What a caller must state to open a terminal, because the connection cannot know it.
 *
 * A session connection knows its host, its session and its wire. It does **not** know which of the
 * session's terminals a pane is showing, nor who currently holds the right to type into it — those
 * belong to the screen, and the daemon refuses a call that gets either wrong. Passing them in is
 * what keeps `TerminalFeed` free of them: once opened, the terminal writes bytes and nothing else.
 */
export interface TerminalOptions {
  /**
   * Which of the session's terminals to open. Empty (the default) resolves to the reserved main
   * ("claude"/Agent) terminal; a bash terminal started with `StartTerminalSession` passes its own
   * id. A session has several, so a connection cannot pick one on the caller's behalf.
   */
  readonly terminalId?: string;

  /**
   * The caller's access token, which the daemon resolves to a GitHub user before serving any
   * terminal RPC.
   *
   * Stated rather than left to `createAuthGateInterceptor` because that gate is **unary-only**
   * (`rpc/authGateInterceptor.ts` — `if (!req.stream && …)`), and every terminal RPC that matters
   * here (`StreamTerminalOutput`, `GetTerminalHistory`) is server-streaming. A feed that relied on
   * the gate would open a stream with an empty token and be refused as unauthenticated.
   */
  readonly sessionToken: string;

  /**
   * The control lease held by the screen doing the typing, read **at send time**.
   *
   * A getter, not a value, because the lease moves: a second screen claiming the terminal replaces
   * it (`ClaimTerminalControl`), and the daemon compares what a `SendTerminalInput` presents against
   * whatever it holds now (`cli_session_manager.rs` — `verify_control`). A token snapshotted when
   * the feed was opened goes stale the moment control changes hands, and every subsequent keystroke
   * comes back `failed_precondition: terminal controlled by another screen` — silently, since input
   * has no reply the terminal renders. Reading it per send is what lets a re-claim resume typing
   * without reopening the stream.
   */
  readonly controlToken: () => string;

  /**
   * The terminal's measured grid, so the daemon resizes the PTY *before* it replays.
   *
   * Omitted when the pane has not been laid out yet. This is the fix for the 220-column garbling:
   * replaying a buffer at the wrong width re-wraps every line in it.
   */
  readonly initialGrid?: { readonly cols: number; readonly rows: number };
}

/**
 * Whether a terminal fed by `feed` can scroll back past what is live.
 *
 * One predicate rather than `feed.history !== undefined` scattered through the component, for the
 * same reason `useHasCapability` exists: the question is asked in several places and must be
 * answered identically in all of them.
 */
export function feedSupportsHistory(feed: TerminalFeed): boolean {
  // The *value*, never `"history" in feed`. A provider that builds its feed conditionally —
  // `{ stream, history: canReplay ? fetcher : undefined }` — produces the key either way, so the
  // membership test would answer `true` for a feed whose scrollback control calls `undefined`.
  return feed.history !== undefined;
}
