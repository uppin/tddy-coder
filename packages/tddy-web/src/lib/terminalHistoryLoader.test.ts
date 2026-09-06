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

/**
 * The fill a terminal drives when its stream carries no offsets at all.
 *
 * A LiveKit-carried session's frames are `terminal.TerminalOutput`, which is `bytes data` and
 * nothing else — there is no `end_offset` to anchor against. That is a different fact from an
 * anchor of zero, which a `SessionTerminalOutput` replay frame reports when the terminal has
 * captured nothing, so the two are constructed differently.
 */
function anUnanchoredLoader(): TerminalHistoryForwardLoader {
  return new TerminalHistoryForwardLoader(null);
}

/** A host that serves `chunks` in order, and records the range each fetch asked for. */
function aHostServing(...chunks: HistoryChunk[]) {
  const asked: Array<{ from: bigint; until: bigint | null }> = [];
  let next = 0;
  return {
    asked,
    fetchChunk: async (from: bigint, until: bigint): Promise<HistoryChunk | null> => {
      asked.push({ from, until });
      return next < chunks.length ? chunks[next++] : null;
    },
  };
}

describe("a forward fill with no offset anchor", () => {
  it("pages forward from the oldest retained byte", async () => {
    // Given a host holding two pages of older output, and a terminal whose stream never told it
    // where the live tail is
    const host = aHostServing(
      aChunk("older-1\n", 0n, 600n, /* atOldest */ true, /* atEnd */ false),
      aChunk("older-2\n", 600n, 1000n, /* atOldest */ false, /* atEnd */ true),
    );
    const loader = anUnanchoredLoader();

    // When the fill runs
    const first = await loader.loadNext(host.fetchChunk);
    const second = await loader.loadNext(host.fetchChunk);

    // Then both pages arrive, in order. An anchor of zero would have made the loader `done` before
    // it asked for anything — which is why "no anchor" is not spelled `0n`.
    expect([first?.data, second?.data]).toEqual([enc("older-1\n"), enc("older-2\n")]);
  });

  it("asks the host for history up to the capture tip", async () => {
    // Given an unanchored fill over two pages
    const host = aHostServing(
      aChunk("older-1\n", 0n, 600n, /* atOldest */ true, /* atEnd */ false),
      aChunk("older-2\n", 600n, 1000n, /* atOldest */ false, /* atEnd */ true),
    );
    const loader = anUnanchoredLoader();

    // When the fill runs
    await loader.loadNext(host.fetchChunk);
    await loader.loadNext(host.fetchChunk);

    // Then each fetch resumes at the previous chunk's end and bounds itself with `0n`, which
    // `GetTerminalHistory` reads as "until the capture tip"
    expect(host.asked).toEqual([
      { from: 0n, until: 0n },
      { from: 600n, until: 0n },
    ]);
  });

  it("terminates on the chunk that reports atEnd", async () => {
    // Given a host whose single page already reaches the capture tip
    const host = aHostServing(aChunk("all of it\n", 0n, 200n, /* atOldest */ true, /* atEnd */ true));
    const loader = anUnanchoredLoader();

    // When the fill runs past that page
    const first = await loader.loadNext(host.fetchChunk);
    const afterTheEnd = await loader.loadNext(host.fetchChunk);

    // Then the page arrived and nothing follows it — with no anchor to compare against, the
    // chunk's own verdict is the only thing that ends the fill
    expect([first?.data, afterTheEnd]).toEqual([enc("all of it\n"), null]);
  });

  it("reports the offset the next fetch resumes from", async () => {
    // Given a host serving a page that does not reach the tip
    const host = aHostServing(
      aChunk("older-1\n", 0n, 600n, /* atOldest */ true, /* atEnd */ false),
      aChunk("older-2\n", 600n, 1000n, /* atOldest */ false, /* atEnd */ true),
    );
    const loader = anUnanchoredLoader();

    // When the first page lands
    const first = await loader.loadNext(host.fetchChunk);

    // Then the page states where the fill resumes — its own end, which is what the caller driving
    // the fill reads back
    expect(first?.nextFromOffset).toBe(600n);
  });
});

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
