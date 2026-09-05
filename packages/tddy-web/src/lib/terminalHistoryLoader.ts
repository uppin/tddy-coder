/**
 * Terminal history forward loader — the client-side state machine for the progressive,
 * append-only forward fill of older terminal output via the `ConnectionService.getTerminalHistory`
 * RPC.
 *
 * docs/ft/web/terminal-replay-lazy-scroll.md
 *
 * The daemon's `StreamTerminalOutput` sends the current last frame first, tagged with its absolute
 * `endOffset` (the anchor). Older history is then filled FORWARD from offset 0 toward the anchor:
 * each `GetTerminalHistory(fromOffset, untilOffset)` call returns one forward chunk starting at
 * `fromOffset`, the client appends it to a second (older-history) ghostty-web terminal, advances
 * `fromOffset` to the chunk's `endOffset`, and calls again until a chunk arrives with `atEnd = true`
 * (reached the anchor). This is a progressive, append-only fill — no resets, no prepend — so the
 * live terminal can stay at `scrollback: 0` (preserving the no-duplicate fix) while the older
 * terminal accumulates history forward.
 */

import type { Client } from "@connectrpc/connect";
import type { ConnectionService } from "../gen/connection_pb";

export interface HistoryChunk {
  data: Uint8Array;
  startOffset: bigint;
  endOffset: bigint;
  atOldest: boolean;
  /** True when this chunk reaches `untilOffset` (or the capture tip) — terminates the forward fill. */
  atEnd: boolean;
}

/**
 * One forward-chunk fetch issued by the loader. Returns `null` when the backend yields no chunk for
 * the given range (treated as reaching the end of retained history).
 */
export type FetchHistoryChunk = (
  fromOffset: bigint,
  untilOffset: bigint,
) => Promise<HistoryChunk | null>;

/**
 * Build a `FetchHistoryChunk` that issues one `ConnectionService.getTerminalHistory` RPC per call,
 * adapting the generated `TerminalHistoryChunk` to the loader's plain `HistoryChunk` shape. Returns
 * `null` when the stream yields no chunk.
 */
export function createForwardHistoryFetcher(
  client: Client<typeof ConnectionService>,
  req: { sessionToken: string; sessionId: string; terminalId: string; maxBytes?: number },
): FetchHistoryChunk {
  return async (fromOffset: bigint, untilOffset: bigint): Promise<HistoryChunk | null> => {
    for await (const chunk of client.getTerminalHistory({
      sessionToken: req.sessionToken,
      sessionId: req.sessionId,
      terminalId: req.terminalId,
      fromOffset,
      untilOffset,
      maxBytes: req.maxBytes ?? 0,
    })) {
      // The backend emits exactly one chunk per call then closes the stream.
      return {
        data: chunk.data,
        startOffset: chunk.startOffset,
        endOffset: chunk.endOffset,
        atOldest: chunk.atOldest,
        atEnd: chunk.atEnd,
      };
    }
    return null;
  };
}

export interface LoadedChunk extends HistoryChunk {
  /** The next `fromOffset` to fetch, or `null` when the forward fill is complete. */
  nextFromOffset: bigint | null;
}

/**
 * Stateful forward-fill loader. Construct with the `untilOffset` (the anchor `endOffset` from the
 * initial `StreamTerminalOutput` frame) and whether that frame already reported `atOldest` (no
 * older history exists). Call `loadNext` repeatedly to fetch and append forward chunks until done.
 *
 * `untilOffset` may also be **`null`**: the terminal's stream carries no offsets at all, so there
 * is no anchor to compare against and the fill runs to the capture tip, ending on the first chunk
 * that reports `atEnd`. That is `terminal.TerminalOutput` — `bytes data` and nothing more — which
 * is what a LiveKit-carried session's frames are. It is deliberately **not** spelled `0n`: a
 * `SessionTerminalOutput` replay frame reports an anchor of zero when the terminal has captured
 * nothing, which means the opposite (there is no history, do not ask for any). Collapsing the two
 * would have this loader page a terminal that has produced no output, and leave a LiveKit session
 * with the no-scrollback behaviour this is here to remove.
 */
export class TerminalHistoryForwardLoader {
  private fromOffset: bigint;
  /** The anchor to fill toward, or `null` when the stream carries no offsets — see the class doc. */
  private readonly untilOffset: bigint | null;
  private reachedEnd = false;

  constructor(untilOffset: bigint | null, atOldest = false) {
    this.untilOffset = untilOffset;
    this.fromOffset = 0n;
    // No older history to fill when a *stated* anchor is empty, or the initial frame already
    // reached the oldest retained byte. An absent anchor states nothing, so it stops nothing.
    if ((untilOffset !== null && untilOffset <= 0n) || atOldest) {
      this.reachedEnd = true;
    }
  }

  /** True once the forward fill has reached the anchor (no more older history to append). */
  get done(): boolean {
    return this.reachedEnd;
  }

  /** The `fromOffset` the next `loadNext` will pass to `fetchChunk`, or `null` when `done`. */
  get pendingFromOffset(): bigint | null {
    return this.done ? null : this.fromOffset;
  }

  /**
   * Fetch the next forward chunk. Returns `null` when `done`. Each call advances the internal
   * `fromOffset` to the returned chunk's `endOffset`, so the next call fetches the chunk that
   * follows it, until `atEnd` (or an empty/null chunk) terminates the fill.
   */
  async loadNext(fetchChunk: FetchHistoryChunk): Promise<LoadedChunk | null> {
    if (this.done) return null;
    // `0n` on the wire is `GetTerminalHistory`'s "until the capture tip", which is exactly what an
    // unanchored fill wants — see `connection.proto`'s `until_offset`.
    const chunk = await fetchChunk(this.fromOffset, this.untilOffset ?? 0n);
    if (chunk === null) {
      this.reachedEnd = true;
      return null;
    }
    // An empty-data chunk, or one that already reports atEnd, means the fill is complete. Reaching
    // the anchor does too — when there is one; unanchored, the chunk's own `atEnd` is the only
    // verdict there is.
    const reachedAnchor = this.untilOffset !== null && chunk.endOffset >= this.untilOffset;
    const ended = chunk.atEnd || chunk.data.length === 0 || reachedAnchor;
    this.fromOffset = chunk.endOffset;
    this.reachedEnd = ended;
    return {
      data: chunk.data,
      startOffset: chunk.startOffset,
      endOffset: chunk.endOffset,
      atOldest: chunk.atOldest,
      atEnd: chunk.atEnd,
      nextFromOffset: this.done ? null : this.fromOffset,
    };
  }
}
