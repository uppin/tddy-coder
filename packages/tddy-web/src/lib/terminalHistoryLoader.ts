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
 */
export class TerminalHistoryForwardLoader {
  private fromOffset: bigint;
  private readonly untilOffset: bigint;
  private reachedEnd = false;

  constructor(untilOffset: bigint, atOldest = false) {
    this.untilOffset = untilOffset;
    this.fromOffset = 0n;
    // No older history to fill when the anchor is empty or the initial frame already reached the
    // oldest retained byte.
    if (untilOffset <= 0n || atOldest) {
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
    const chunk = await fetchChunk(this.fromOffset, this.untilOffset);
    if (chunk === null) {
      this.reachedEnd = true;
      return null;
    }
    // An empty-data chunk, or one that already reports atEnd, means the fill is complete.
    const ended = chunk.atEnd || chunk.data.length === 0 || chunk.endOffset >= this.untilOffset;
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
