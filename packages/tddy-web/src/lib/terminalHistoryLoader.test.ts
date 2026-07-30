/**
 * Unit tests — `TerminalHistoryForwardLoader` (progressive forward-fill state machine).
 *
 * docs/ft/web/terminal-replay-lazy-scroll.md
 *
 * The loader drives a forward, append-only fill of older history: each `loadNext` fetches one
 * forward chunk starting at the current `fromOffset` (bounded by the anchor `untilOffset`), then
 * advances `fromOffset` to the chunk's `endOffset`, stopping at `atEnd`. These tests record usage
 * over a fake fetch — no real RPC, no wire format.
 */

import { describe, expect, it } from "bun:test";
import { TerminalHistoryForwardLoader, type HistoryChunk } from "./terminalHistoryLoader";

const enc = (s: string): Uint8Array => new TextEncoder().encode(s);

function aChunk(
  data: string,
  startOffset: bigint,
  endOffset: bigint,
  atOldest: boolean,
  atEnd: boolean,
): HistoryChunk {
  return {
    data: enc(data),
    startOffset,
    endOffset,
    atOldest,
    atEnd,
  };
}

describe("TerminalHistoryForwardLoader", () => {
  it("is done immediately when the anchor is zero (no captured history)", () => {
    const loader = new TerminalHistoryForwardLoader(0n);
    expect(loader.done).toBe(true);
    expect(loader.pendingFromOffset).toBe(null);
  });

  it("is done immediately when the initial frame reported atOldest (no older history)", () => {
    const loader = new TerminalHistoryForwardLoader(1000n, /* atOldest */ true);
    expect(loader.done).toBe(true);
    expect(loader.pendingFromOffset).toBe(null);
  });

  it("fetches forward chunks from offset 0, advancing fromOffset to each chunk's endOffset", async () => {
    // Given an anchor at 1000 and two forward chunks tiling 0..1000.
    const calls: Array<{ from: bigint; until: bigint }> = [];
    const fetchChunk = async (from: bigint, until: bigint): Promise<HistoryChunk | null> => {
      calls.push({ from, until });
      if (from === 0n) return aChunk("older-1\n", 0n, 600n, true, false);
      if (from === 600n) return aChunk("older-2\n", 600n, 1000n, false, true);
      return null;
    };

    const loader = new TerminalHistoryForwardLoader(1000n);
    expect(loader.pendingFromOffset).toBe(0n);

    const first = await loader.loadNext(fetchChunk);
    expect(first).not.toBeNull();
    expect(first!.data).toEqual(enc("older-1\n"));
    expect(first!.startOffset).toBe(0n);
    expect(first!.endOffset).toBe(600n);
    expect(first!.atEnd).toBe(false);
    expect(first!.nextFromOffset).toBe(600n);
    expect(loader.done).toBe(false);
    expect(calls).toEqual([{ from: 0n, until: 1000n }]);

    const second = await loader.loadNext(fetchChunk);
    expect(second!.data).toEqual(enc("older-2\n"));
    expect(second!.atEnd).toBe(true);
    expect(second!.nextFromOffset).toBe(null);
    expect(loader.done).toBe(true);
    expect(calls).toEqual([
      { from: 0n, until: 1000n },
      { from: 600n, until: 1000n },
    ]);

    // Subsequent calls are no-ops.
    const again = await loader.loadNext(fetchChunk);
    expect(again).toBe(null);
  });

  it("stops loading once a chunk reports atEnd", async () => {
    const fetchChunk = async (_from: bigint, _until: bigint): Promise<HistoryChunk | null> =>
      aChunk("only\n", 0n, 200n, true, true);

    const loader = new TerminalHistoryForwardLoader(200n);
    const first = await loader.loadNext(fetchChunk);
    expect(first!.atEnd).toBe(true);
    expect(loader.done).toBe(true);

    const again = await loader.loadNext(fetchChunk);
    expect(again).toBe(null);
  });

  it("treats a null fetch result as reaching the end of retained history", async () => {
    const fetchChunk = async (_from: bigint, _until: bigint): Promise<HistoryChunk | null> => null;
    const loader = new TerminalHistoryForwardLoader(300n);
    const first = await loader.loadNext(fetchChunk);
    expect(first).toBe(null);
    expect(loader.done).toBe(true);
  });

  it("treats an empty-data chunk as the end of retained history", async () => {
    const fetchChunk = async (_from: bigint, _until: bigint): Promise<HistoryChunk | null> =>
      aChunk("", 300n, 300n, true, true);
    const loader = new TerminalHistoryForwardLoader(300n);
    const first = await loader.loadNext(fetchChunk);
    expect(first!.data.length).toBe(0);
    expect(loader.done).toBe(true);
  });
});
