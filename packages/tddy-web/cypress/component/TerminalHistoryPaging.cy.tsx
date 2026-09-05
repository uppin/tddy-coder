/**
 * Acceptance tests: the terminal's overlay double-buffer scroll-up history.
 *
 * PRD: docs/ft/web/terminal-replay-lazy-scroll.md (amended — terminal-native-scrolling)
 *
 * The Ghostty shared component owns the paging flow with two interchangeable, overlayed ghostty-web
 * terminals sharing one rect. The LIVE terminal carries native scrollback > 0 (native primary-screen
 * behavior) and accumulates post-connect output; ghostty-web's alternate buffer (DEC 1049) has no
 * scrollback, so a proper TUI that switches to the alternate screen never accumulates duplicate panes
 * — exactly as native ghostty does. The PAGE terminal (scrollback > 0) holds the forward-filled older
 * history and exposes a native Scrollbar {total, offset, len} as the single source of truth for
 * viewport position (same coordinate space as scrollToLine). On a scroll-up-at-top-of-live gesture
 * (or the "Load earlier output" affordance) the page terminal is forward-filled from offset 0 toward
 * the anchor while a loading indicator is shown; once the fill completes the two terminals switch
 * places. "Back to live" (or a scroll-down-at-bottom gesture on the page terminal) swaps back. The
 * scroll-to-bottom policy matches native defaults: keystroke = yes, output = no. Mouse tracking (DEC
 * 1006) gates the wheel to the TUI instead of the viewport. All paging logic is encapsulated here.
 */

import {
  aTerminalWithHistoryPaging,
  aReplayFrame,
} from "../support/drivers/terminalHistoryPagingDriver";
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

describe("Terminal history paging — overlay double-buffer", () => {
  it("shows the load-earlier-history affordance when the initial frame carries endOffset and atOldest is false", () => {
    // Given / When — the terminal mounts and the initial replay frame arrives
    // Then — the affordance is visible (older history is available) and the live pane is foreground
    aTerminalWithHistoryPaging()
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
    aTerminalWithHistoryPaging()
      .mount()
      .expectReady()
      .pushFrame(atOldestFrame)
      .expectAffordanceAbsent();
  });

  it("shows the loading indicator and fetches forward from offset 0 when the affordance is activated", () => {
    // Given
    const terminal = aTerminalWithHistoryPaging()
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
    const terminal = aTerminalWithHistoryPaging()
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
    const terminal = aTerminalWithHistoryPaging()
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
    const terminal = aTerminalWithHistoryPaging()
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
    const terminal = aTerminalWithHistoryPaging()
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
    const terminal = aTerminalWithHistoryPaging()
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
    const terminal = aTerminalWithHistoryPaging()
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
    const terminal = aTerminalWithHistoryPaging()
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
    const terminal = aTerminalWithHistoryPaging()
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

  // -------------------------------------------------------------------------
  // Scrollback-0 model (live terminal pinned to the tip; first scroll-up loads history)
  // -------------------------------------------------------------------------

  it("live terminal has no native scrollback — overflow output does not accumulate, and a scroll-up gesture triggers the page forward-fill immediately", () => {
    // Given — the live terminal carries scrollback 0 (always pinned to the live tip).
    const terminal = aTerminalWithHistoryPaging()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);

    // When — push ~40 lines of output (rows is 24, so a native-scrollback terminal would accumulate
    // ~16 scrollback lines above the active screen).
    const lines = Array.from({ length: 40 }, (_, i) => `live-line-${i}\n`).join("");
    const liveBytes = BigInt(enc(lines).length);
    terminal.pushOutput(enc(lines));

    // Then — the live terminal accumulated NO native scrollback (scrollback 0 ⇒ always pinned to
    // the tip) and the viewport stayed at the bottom (viewportY 0).
    terminal.expectLiveScrollbackLength(0).expectLiveViewportY(0);

    // When — the user performs a scroll-up wheel gesture on the live pane (pinned to the bottom)
    terminal.scrollUpAtTop();

    // Then — the loading indicator is shown and a forward fetch is pending immediately (no
    // intermediate "scroll through live scrollback" step with scrollback 0). The fetch is bounded by
    // the CURRENT live tip (anchor + post-connect bytes), not the stale anchor — so the page fills
    // with the full retained history rather than an evicted-empty range.
    terminal
      .expectLoadingVisible()
      .expectHistoryFetchPending()
      .expectHistoryFetchCalledWith(0n, ANCHOR + liveBytes);
  });

  it("after a reconnect that advances the tip past the original anchor, a scroll-up fills the page with the retained history (no blank page)", () => {
    // Given — the live terminal opened at anchor 1000, then reconnected: a FROM_OFFSET catch-up
    // frame snaps currentOffset to a NEW tip (2000) past the original anchor, simulating the ring
    // evicting the original anchor. The page forward-fill must be bounded by the CURRENT tip, not
    // the stale anchor — otherwise `replay_from(0, 1000)` is an evicted-empty range and the page
    // would swap to blank.
    const terminal = aTerminalWithHistoryPaging()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME) // anchor captured at 1000
      .pushFrame(aReplayFrame(enc("CATCHUP\r\n"), 2000n, /* atOldest */ false)); // reconnect catch-up → tip 2000

    // When — the user scrolls up on the live pane (pinned to the bottom).
    terminal.scrollUpAtTop();

    // Then — the forward fetch is bounded by the CURRENT tip (2000), NOT the stale anchor (1000),
    // so the daemon returns the retained history `[start_offset, 2000]` instead of an empty range.
    terminal
      .expectLoadingVisible()
      .expectHistoryFetchPending()
      .expectHistoryFetchCalledWith(0n, 2000n);

    // When — the daemon resolves with the retained history chunk (start_offset 1500 → tip 2000,
    // i.e. the original anchor 1000 has been evicted; only [1500, 2000] is retained).
    terminal.resolveHistoryChunk({
      data: enc("retained-line-0\r\nretained-line-1\r\n"),
      startOffset: 1500n,
      endOffset: 2000n,
      atOldest: true,
      atEnd: true,
    });

    // Then — the page terminal swaps to the foreground and SHOWS the retained history (not a blank
    // page). The live pane stays mounted underneath.
    terminal
      .expectPageForeground()
      .expectOlderBufferContainsInOrder("retained-line-0", "retained-line-1")
      .expectNoFurtherHistoryFetch();
  });

  it("a forward fill that resolves with no retained bytes (empty range) stays on the live pane — no blank page", () => {
    // Given — the live terminal opened at anchor 1000 with older history available.
    const terminal = aTerminalWithHistoryPaging()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);

    // When — the user scrolls up and the daemon yields no chunk (the requested range is empty /
    // evicted below the ring's start_offset).
    terminal.scrollUpAtTop().expectHistoryFetchPending().resolveHistoryNull();

    // Then — the loading indicator is gone, the live pane stays foreground (NOT a blank page),
    // and the fill is not marked complete so the affordance remains available for a later retry.
    terminal
      .expectLoadingAbsent()
      .expectLiveForeground()
      .expectAffordanceVisible()
      .expectNoFurtherHistoryFetch();
  });

  it("a forward fill whose fetch errors (e.g. getTerminalHistory not_found) stays on the live pane — no blank page", () => {
    // Given — the live terminal opened at anchor 1000 with older history available.
    const terminal = aTerminalWithHistoryPaging()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);

    // When — the user scrolls up and the fetch rejects (RPC error, e.g. a session type whose
    // `getTerminalHistory` is not supported).
    terminal
      .scrollUpAtTop()
      .expectHistoryFetchPending()
      .rejectHistoryFetch(new Error("not found: terminal not found or not running"));

    // Then — the loading indicator is gone, the live pane stays foreground (NOT a blank page),
    // and the affordance remains available.
    terminal
      .expectLoadingAbsent()
      .expectLiveForeground()
      .expectAffordanceVisible()
      .expectNoFurtherHistoryFetch();
  });

  it("live terminal stays at scrollback 0 across primary and alternate screen (DEC 1049) — no native scrollback to duplicate", () => {
    // Given — the live terminal carries scrollback 0 (always pinned to the live tip); push enough
    // output that a native-scrollback terminal would have accumulated history.
    const terminal = aTerminalWithHistoryPaging()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);
    const baseline = Array.from({ length: 30 }, (_, i) => `base-line-${i}\n`).join("");
    terminal.pushOutput(enc(baseline));

    // When — capture the live scrollback length, then enter the alternate screen (DEC 1049),
    // repaint the full screen, and exit back to the primary screen.
    terminal.captureLiveScrollbackLength().then((before) => {
      expect(before, "live scrollback is 0 (no native scrollback)").to.equal(0);
      const altRepaint =
        "\x1b[?1049h" + // enter alternate screen
        Array.from({ length: 24 }, (_, i) => `\x1b[${i + 1};1Halt-row-${i}`).join("\r\n") +
        "\x1b[?1049l"; // exit alternate screen
      terminal.pushOutput(enc(altRepaint));

      // Then — the live scrollback is still 0: with scrollback 0 there is no native retention at
      // all (primary or alternate), so a TUI repaint cannot leave duplicate panes behind.
      terminal.expectLiveScrollbackLength(0);
    });
  });

  it("page terminal exposes the native Scrollbar {total, offset, len} as the single source of truth and scrollToLine sets the absolute offset", () => {
    // Given — the page terminal has been forward-filled with a 30-line older-history chunk
    // (scrollback > rows), so the native Scrollbar has real history to report.
    const terminal = aTerminalWithHistoryPaging()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME)
      .expectAffordanceVisible()
      .activateLoadEarlier()
      .expectLoadingVisible()
      .expectHistoryFetchPending()
      .resolveHistoryChunk(OLDER_CHUNK_DEEP)
      .expectPageForeground();

    // Sanity — landed at the bottom (the seam) after the fill: offset = total - len.
    terminal.capturePageScrollbar().then((atBottom) => {
      expect(atBottom.total, "page scrollbar total").to.be.greaterThan(0);
      expect(atBottom.offset, "page scrollbar offset at bottom").to.equal(atBottom.total - atBottom.len);

      // When — scroll the page viewport to an absolute line at the top via scrollToLine.
      terminal.scrollPageToLine(0);

      // Then — the native Scrollbar offset is now 0 (top of scrollback), total and len unchanged.
      terminal.expectPageScrollbar({
        total: atBottom.total,
        offset: 0,
        len: atBottom.len,
      });
    });
  });

  it("mouse tracking on (DEC 1006) routes the wheel to the TUI, not the viewport (no forward-fill, viewport stays pinned)", () => {
    // Given — the live terminal has scrollback 0 (always pinned to the live tip); enable mouse
    // tracking (DEC 1006) so the wheel is reported to the TUI instead of triggering forward-fill.
    const terminal = aTerminalWithHistoryPaging()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);
    const lines = Array.from({ length: 40 }, (_, i) => `live-line-${i}\n`).join("");
    terminal.pushOutput(enc(lines));
    terminal.expectLiveViewportY(0); // scrollback 0 ⇒ always pinned to the bottom
    terminal.enableLiveMouseTracking(true);

    // When — wheel up on the live pane while mouse tracking is on.
    terminal.scrollUpAtTop();

    // Then — the live viewport is still 0 (scrollback 0, pinned) and NO forward-fill fired: the
    // wheel was routed to the TUI, so the lazy-history gesture is suppressed.
    terminal.expectLiveViewportY(0).expectNoFurtherHistoryFetch();
  });

  // -------------------------------------------------------------------------
  // Three-way wheel gate: mouse tracking × alternate screen × normal screen
  // (restores the native ghostty gate that ghostty-web's handleWheel drops —
  // it checks isAlternateScreen() but NOT hasMouseTracking(), so a mouse-tracked
  // TUI like Claude CLI gets Up/Down arrows instead of SGR mouse events.)
  // -------------------------------------------------------------------------

  it("mouse tracking on + alternate screen routes wheel-up to the TUI as an SGR mouse event, not an Up-arrow key (no prompt-history recall)", () => {
    // Given — the live terminal is in the alternate screen (DEC 1049, like Claude CLI's TUI) and the
    // TUI has enabled mouse tracking (DEC 1006). ghostty-web's handleWheel unconditionally emits
    // Up/Down arrows in the alternate screen — but when the TUI opted into mouse tracking, the wheel
    // must be reported as an SGR mouse event (button 64 = wheel up), never as an arrow key.
    const terminal = aTerminalWithHistoryPaging()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);
    terminal.enterLiveAltScreen();
    terminal.enableLiveMouseTracking(true);

    // When — wheel up on the live pane.
    terminal.scrollUpAtTop();

    // Then — the PTY received an SGR wheel-up mouse report and did NOT receive an Up-arrow key
    // (\x1b[A), and no forward-fill was triggered (the wheel belongs to the TUI, not the lazy history).
    terminal
      .expectLiveSentIncludes("\x1b[<64;")
      .expectLiveDidNotSend("\x1b[A")
      .expectNoHistoryFetchStarted();
  });

  it("mouse tracking on + alternate screen routes wheel-down to the TUI as an SGR mouse event, not a Down-arrow key", () => {
    // Given — alternate screen (DEC 1049) + mouse tracking on (DEC 1006), same as Claude CLI.
    const terminal = aTerminalWithHistoryPaging()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);
    terminal.enterLiveAltScreen();
    terminal.enableLiveMouseTracking(true);

    // When — wheel down on the live pane.
    terminal.scrollDownOnLive();

    // Then — the PTY received an SGR wheel-down mouse report (button 65) and did NOT receive a
    // Down-arrow key (\x1b[B), and no forward-fill was triggered.
    terminal
      .expectLiveSentIncludes("\x1b[<65;")
      .expectLiveDidNotSend("\x1b[B")
      .expectNoHistoryFetchStarted();
  });

  it("mouse tracking off + alternate screen does NOT trigger forward-fill on wheel-up (native arrow emulation for pagers like `less` is preserved)", () => {
    // Given — the live terminal is in the alternate screen (DEC 1049) but the TUI has NOT enabled
    // mouse tracking (e.g. `less`/`man`). Native terminals emulate Up/Down arrows for the wheel in
    // this mode so the pager scrolls — our forward-fill must NOT hijack that gesture.
    const terminal = aTerminalWithHistoryPaging()
      .mount()
      .expectReady()
      .pushFrame(REPLAY_FRAME);
    terminal.enterLiveAltScreen();
    // mouse tracking stays OFF (default)

    // When — wheel up on the live pane.
    terminal.scrollUpAtTop();

    // Then — no forward-fill fetch is started (the wheel is left to ghostty-web's native
    // alternate-screen arrow emulation so the pager scrolls instead of triggering lazy history).
    terminal.expectNoHistoryFetchStarted();
  });
});
