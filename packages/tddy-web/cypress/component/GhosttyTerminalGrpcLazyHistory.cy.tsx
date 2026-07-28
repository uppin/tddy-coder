/**
 * Acceptance tests: GhosttyTerminalGrpc progressive forward-fill scroll-up history.
 *
 * PRD: docs/ft/web/terminal-replay-lazy-scroll.md
 *
 * The Ghostty shared component owns the scroll-up flow: it captures the `endOffset` anchor from the
 * initial `StreamTerminalOutput` frame, shows a "Load earlier output" affordance while older
 * history exists, and progressively fills a second, read-only ghostty-web terminal FORWARD from
 * offset 0 toward the anchor via the `historyFetcher` prop (one `GetTerminalHistory`-shaped call
 * per chunk, advancing `fromOffset` until `atEnd`). No resets; the live terminal stays at
 * `scrollback: 0`.
 */

import {
  aGhosttyTerminalGrpcLazyHistory,
  aReplayFrame,
} from "../support/drivers/ghosttyTerminalGrpcLazyHistoryDriver";
import type { HistoryChunk } from "../../src/lib/terminalHistoryLoader";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ANCHOR = 1000n;
const enc = (s: string): Uint8Array => new TextEncoder().encode(s);

/** The initial replay frame: last-screen bytes tagged with the anchor. */
const REPLAY_FRAME = aReplayFrame(enc("LIVE\r\n"), ANCHOR, /* atOldest */ false);

/** First forward chunk (0..600), not atEnd — the fill continues. */
const OLDER_CHUNK_FIRST: HistoryChunk = {
  data: enc("OLDER-1\r\n"),
  startOffset: 0n,
  endOffset: 600n,
  atOldest: true,
  atEnd: false,
};

/** Final forward chunk (600..1000), atEnd — terminates the fill at the anchor. */
const OLDER_CHUNK_FINAL: HistoryChunk = {
  data: enc("OLDER-2\r\n"),
  startOffset: 600n,
  endOffset: ANCHOR,
  atOldest: false,
  atEnd: true,
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("GhosttyTerminalGrpcLazyHistory — progressive forward-fill viewport integration", () => {
  it("shows the load-earlier-history affordance when the initial frame carries endOffset and atOldest is false", () => {
    // Given / When — the terminal mounts and the initial replay frame arrives
    // Then — the affordance is visible (older history is available)
    aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME)
      .expectAffordanceVisible();
  });

  it("hides the load-earlier-history affordance when the initial frame reports atOldest", () => {
    // Given — the initial frame reports the capture ring is already at its oldest byte
    const atOldestFrame = aReplayFrame(enc("LIVE\r\n"), ANCHOR, true);

    // When — the terminal mounts and the at-oldest replay frame arrives
    // Then — no affordance (there is no older history to load)
    aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(atOldestFrame)
      .expectAffordanceAbsent();
  });

  it("fetches older history forward from offset 0, bounded by the anchor, when the affordance is activated", () => {
    // Given
    const terminal = aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);

    // When — the user activates the affordance
    terminal
      .expectAffordanceVisible()
      .activateLoadEarlier();

    // Then — a history fetch is pending, anchored forward from 0 to the anchor
    terminal
      .expectHistoryFetchPending()
      .expectHistoryFetchCalledWith(0n, ANCHOR);
  });

  it("appends older bytes to the older-history terminal and chains the next forward chunk", () => {
    // Given
    const terminal = aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);

    // When — activate, then the first fetch resolves with a non-atEnd chunk
    terminal
      .expectAffordanceVisible()
      .activateLoadEarlier()
      .expectHistoryFetchPending()
      .expectHistoryFetchCalledWith(0n, ANCHOR)
      .resolveHistoryChunk(OLDER_CHUNK_FIRST);

    // Then — the older terminal holds the first chunk's bytes, and a second fetch is issued
    // forward from the chunk's endOffset (600) toward the anchor.
    terminal
      .expectOlderBufferContainsInOrder("OLDER-1")
      .expectHistoryFetchPending()
      .expectHistoryFetchCalledWith(600n, ANCHOR);
  });

  it("stops fetching once a chunk reports atEnd (reached the anchor)", () => {
    // Given
    const terminal = aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);

    // When — the fill runs to completion: first chunk (not atEnd), then final chunk (atEnd)
    terminal
      .expectAffordanceVisible()
      .activateLoadEarlier()
      .expectHistoryFetchPending()
      .resolveHistoryChunk(OLDER_CHUNK_FIRST)
      .expectHistoryFetchPending()
      .resolveHistoryChunk(OLDER_CHUNK_FINAL);

    // Then — no further fetch is issued and the older terminal holds both chunks in order
    terminal
      .expectNoFurtherHistoryFetch()
      .expectOlderBufferContainsInOrder("OLDER-1", "OLDER-2");
  });

  it("keeps live bytes flowing to the live terminal during the forward fill (no reset, no interruption)", () => {
    // Given
    const terminal = aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);

    // When — the fill is in flight (fetch pending) and live bytes arrive on the stream
    terminal
      .expectAffordanceVisible()
      .activateLoadEarlier()
      .expectHistoryFetchPending()
      .pushOutput(enc("DURING\r\n"))
      .resolveHistoryChunk(OLDER_CHUNK_FINAL);

    // Then — the live terminal received the during-fill bytes (and the initial live frame),
    // and the older terminal received the older chunk. No reset, no buffering, no loss.
    terminal
      .expectLiveBufferContainsInOrder("LIVE", "DURING")
      .expectOlderBufferContainsInOrder("OLDER-2");
  });

  it("triggers the forward fill on a scroll-up-at-top gesture", () => {
    // Given
    const terminal = aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);

    // When — the user scrolls up while pinned to the bottom (no scrollback yet to scroll through)
    terminal.scrollUpAtTop();

    // Then — a history fetch is pending, anchored forward from 0 to the anchor
    terminal
      .expectHistoryFetchPending()
      .expectHistoryFetchCalledWith(0n, ANCHOR);
  });
});
