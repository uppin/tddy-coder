/**
 * Acceptance tests: GhosttyTerminalGrpc overlay double-buffer scroll-up history.
 *
 * PRD: docs/ft/web/terminal-replay-lazy-scroll.md
 *
 * The Ghostty shared component owns the paging flow with two interchangeable, overlayed ghostty-web
 * terminals sharing one rect. The live terminal (scrollback 0) always stays mounted and keeps
 * receiving the stream — scrollback 0 avoids the duplicate-pane accumulation from periodic TUI
 * full-screen re-paints (guarded by the ghostty-fullscreen-no-duplicate e2e test). On a scroll-up-at-top
 * gesture (or the "Load earlier output" affordance) the background page terminal (scrollback > 0) is
 * forward-filled from offset 0 toward the anchor while a loading indicator is shown; once the fill
 * completes the two terminals switch places (the page terminal becomes foreground, scrollable
 * through history with full viewport control via scrollToLine). "Back to live" (or a scroll-down-at-bottom
 * gesture on the page terminal) swaps back. All paging logic is encapsulated inside the component.
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

/** A 30-line older-history chunk delivered in one atEnd chunk — gives the page terminal real
 *  scrollback so scrollToLine can position the viewport within it. */
const OLDER_CHUNK_DEEP: HistoryChunk = {
  data: enc(Array.from({ length: 30 }, (_, i) => `older-line-${i}\n`).join("")),
  startOffset: 0n,
  endOffset: ANCHOR,
  atOldest: true,
  atEnd: true,
};

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("GhosttyTerminalGrpcLazyHistory — overlay double-buffer paging", () => {
  it("shows the load-earlier-history affordance when the initial frame carries endOffset and atOldest is false", () => {
    // Given / When — the terminal mounts and the initial replay frame arrives
    // Then — the affordance is visible (older history is available) and the live pane is foreground
    aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME)
      .expectAffordanceVisible()
      .expectLiveForeground();
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

  it("shows the loading indicator and fetches forward from offset 0 when the affordance is activated", () => {
    // Given
    const terminal = aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);

    // When — the user activates the affordance
    terminal.expectAffordanceVisible().activateLoadEarlier();

    // Then — the loading indicator is shown and a forward fetch is pending (0 → anchor)
    terminal
      .expectLoadingVisible()
      .expectHistoryFetchPending()
      .expectHistoryFetchCalledWith(0n, ANCHOR);
  });

  it("appends older bytes to the background page terminal and chains the next forward chunk", () => {
    // Given
    const terminal = aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);

    // When — activate, then the first fetch resolves with a non-atEnd chunk
    terminal
      .expectAffordanceVisible()
      .activateLoadEarlier()
      .expectLoadingVisible()
      .expectHistoryFetchPending()
      .expectHistoryFetchCalledWith(0n, ANCHOR)
      .resolveHistoryChunk(OLDER_CHUNK_FIRST);

    // Then — the page terminal holds the first chunk's bytes, and a second fetch is issued
    // forward from the chunk's endOffset (600) toward the anchor.
    terminal
      .expectOlderBufferContainsInOrder("OLDER-1")
      .expectHistoryFetchPending()
      .expectHistoryFetchCalledWith(600n, ANCHOR);
  });

  it("swaps the page terminal to the foreground once a chunk reports atEnd (reached the anchor)", () => {
    // Given
    const terminal = aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);

    // When — the fill runs to completion: first chunk (not atEnd), then final chunk (atEnd)
    terminal
      .expectAffordanceVisible()
      .activateLoadEarlier()
      .expectLoadingVisible()
      .expectHistoryFetchPending()
      .resolveHistoryChunk(OLDER_CHUNK_FIRST)
      .expectHistoryFetchPending()
      .resolveHistoryChunk(OLDER_CHUNK_FINAL);

    // Then — no further fetch is issued, the loading indicator is gone, the page terminal holds
    // both chunks in order, and the page pane is now foreground (live pane hidden underneath).
    terminal
      .expectNoFurtherHistoryFetch()
      .expectLoadingAbsent()
      .expectOlderBufferContainsInOrder("OLDER-1", "OLDER-2")
      .expectPageForeground()
      .expectBackToLiveVisible();
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
      .expectLoadingVisible()
      .expectHistoryFetchPending()
      .pushOutput(enc("DURING\r\n"))
      .resolveHistoryChunk(OLDER_CHUNK_FINAL);

    // Then — the live terminal received the during-fill bytes (and the initial live frame),
    // and the page terminal received the older chunk. No reset, no buffering, no loss.
    terminal
      .expectLiveBufferContainsInOrder("LIVE", "DURING")
      .expectOlderBufferContainsInOrder("OLDER-2");
  });

  it("triggers the forward fill on a scroll-up-at-top gesture on the live pane", () => {
    // Given
    const terminal = aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);

    // When — the user scrolls up while pinned to the bottom (no scrollback yet to scroll through)
    terminal.scrollUpAtTop();

    // Then — the loading indicator is shown and a forward fetch is pending (0 → anchor)
    terminal
      .expectLoadingVisible()
      .expectHistoryFetchPending()
      .expectHistoryFetchCalledWith(0n, ANCHOR);
  });

  it("swaps back to the live pane on the Back-to-live affordance, then re-views history instantly", () => {
    // Given — the fill has completed and the page pane is foreground
    const terminal = aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME)
      .expectAffordanceVisible()
      .activateLoadEarlier()
      .expectLoadingVisible()
      .expectHistoryFetchPending()
      .resolveHistoryChunk(OLDER_CHUNK_FINAL)
      .expectPageForeground();

    // When — the user activates "Back to live"
    terminal.activateBackToLive();

    // Then — the live pane is foreground again, the "View history" affordance is shown, and no
    // new fetch is issued (the page terminal is already filled — re-swap is instant).
    terminal
      .expectLiveForeground()
      .expectViewHistoryVisible()
      .expectNoFurtherHistoryFetch();

    // When — the user re-views history
    terminal.activateViewHistory();

    // Then — the page pane is foreground again, instantly, with no new fetch
    terminal.expectPageForeground().expectNoFurtherHistoryFetch();
  });

  it("swaps back to live on a scroll-down-at-bottom gesture on the page pane", () => {
    // Given — the fill has completed and the page pane is foreground
    const terminal = aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME)
      .expectAffordanceVisible()
      .activateLoadEarlier()
      .expectLoadingVisible()
      .expectHistoryFetchPending()
      .resolveHistoryChunk(OLDER_CHUNK_FINAL)
      .expectPageForeground();

    // When — the user scrolls down while pinned to the bottom of the page pane
    terminal.scrollDownAtBottom();

    // Then — the live pane is foreground again
    terminal.expectLiveForeground();
  });

  it("swaps to the page pane instantly on a scroll-up gesture once history is already filled", () => {
    // Given — the fill has completed and the user has returned to the live pane
    const terminal = aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME)
      .expectAffordanceVisible()
      .activateLoadEarlier()
      .expectLoadingVisible()
      .expectHistoryFetchPending()
      .resolveHistoryChunk(OLDER_CHUNK_FINAL)
      .expectPageForeground()
      .activateBackToLive()
      .expectLiveForeground();

    // When — the user scrolls up on the live pane again (history already filled)
    terminal.scrollUpAtTop();

    // Then — the page pane swaps to the foreground instantly, with no new fetch
    terminal.expectPageForeground().expectNoFurtherHistoryFetch();
  });

  it("positions the page terminal viewport via scrollLines (full control of the viewport position)", () => {
    // Given — the page terminal has been forward-filled with a 30-line older-history chunk
    // (scrollback > rows), so scrollLines has real history to scroll through. The page pane is
    // foreground and pinned to the bottom (viewportY 0) at the seam.
    const terminal = aGhosttyTerminalGrpcLazyHistory()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME)
      .expectAffordanceVisible()
      .activateLoadEarlier()
      .expectLoadingVisible()
      .expectHistoryFetchPending()
      .resolveHistoryChunk(OLDER_CHUNK_DEEP)
      .expectPageForeground();

    // Sanity — landed at the bottom (the seam) after the fill.
    terminal.expectPageViewportY(0);

    // When — scroll the page viewport up by 5 lines via the imperative scrollLines handle.
    terminal.scrollPageUp(5);

    // Then — the viewport is positioned exactly 5 lines up from the bottom (full control).
    terminal.expectPageViewportY(5);

    // When — scroll back down by 5 lines (positive = down toward the bottom).
    terminal.scrollPageUp(-5);

    // Then — the viewport is pinned back to the bottom (viewportY 0).
    terminal.expectPageViewportY(0);
  });
});
