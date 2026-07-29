/**
 * Fluent component driver for GhosttyTerminalGrpc overlay double-buffer scroll-up history.
 *
 * Wraps mount → push frames → activate affordance → assert on the live/page buffers, fetcher
 * calls, foreground pane, loading indicator, and swap actions, so test bodies stay free of raw
 * selectors, fake-stream wiring, and promise plumbing.
 *
 * PRD: docs/ft/web/terminal-replay-lazy-scroll.md
 */

import React from "react";
import { mount } from "cypress/react";
import { GhosttyTerminalGrpc, type GrpcStream } from "../../../src/components/GhosttyTerminalGrpc";
import type { HistoryChunk } from "../../../src/lib/terminalHistoryLoader";
import { UploadProgressProvider } from "../../../src/rpc/uploadProgress";
import { byTestId, TEST_IDS } from "../testIds";

/** UTF-8 encode a string into a Uint8Array (driver-local; the spec has its own `enc`). */
const enc = (s: string): Uint8Array => new TextEncoder().encode(s);

// ---------------------------------------------------------------------------
// Frame shape carried by GrpcStream.onMessage (the full SessionTerminalOutput frame)
// ---------------------------------------------------------------------------

export interface GrpcFrame {
  data: Uint8Array;
  endOffset: bigint;
  atOldest: boolean;
}

/** Build a live-tail frame (offsets zeroed). */
export function aLiveFrame(data: Uint8Array): GrpcFrame {
  return { data, endOffset: 0n, atOldest: false };
}

/** Build the initial replay frame tagged with the anchor. */
export function aReplayFrame(data: Uint8Array, endOffset: bigint, atOldest = false): GrpcFrame {
  return { data, endOffset, atOldest };
}

/** Concatenate an array of Uint8Array chunks into a single Uint8Array (for decoding sent PTY bytes). */
function concatUint8(chunks: Uint8Array[]): Uint8Array {
  let total = 0;
  for (const c of chunks) total += c.length;
  const out = new Uint8Array(total);
  let off = 0;
  for (const c of chunks) {
    out.set(c, off);
    off += c.length;
  }
  return out;
}

// ---------------------------------------------------------------------------
// Controllable historyFetcher double (forward fill: fromOffset → untilOffset)
// ---------------------------------------------------------------------------

export interface FetchCall {
  from: bigint;
  until: bigint;
}

export interface FetcherDouble {
  /** The fetcher function passed to the component. */
  fetch: (fromOffset: bigint, untilOffset: bigint) => Promise<HistoryChunk | null>;
  /** Calls observed so far. */
  calls: FetchCall[];
  /** Resolve the in-flight fetch with a chunk. Fails the test if no fetch is pending. */
  resolveChunk: (chunk: HistoryChunk) => void;
  /** Resolve the in-flight fetch with null (no older bytes). */
  resolveNull: () => void;
  /** Reject the in-flight fetch (simulates an RPC error, e.g. `getTerminalHistory` not_found). */
  rejectFetch: (err: unknown) => void;
  /** True when a fetch is awaiting resolution. */
  hasPending: () => boolean;
}

function aFetcherDouble(): FetcherDouble {
  const calls: FetchCall[] = [];
  let pending: ((chunk: HistoryChunk | null) => void) | null = null;
  let pendingReject: ((err: unknown) => void) | null = null;
  const fetch = (fromOffset: bigint, untilOffset: bigint): Promise<HistoryChunk | null> => {
    calls.push({ from: fromOffset, until: untilOffset });
    return new Promise<HistoryChunk | null>((resolve, reject) => {
      pending = resolve;
      pendingReject = reject;
    });
  };
  const clear = () => {
    pending = null;
    pendingReject = null;
  };
  return {
    fetch,
    calls,
    resolveChunk(chunk) {
      if (!pending) throw new Error("no pending historyFetcher call to resolve");
      const resolve = pending;
      clear();
      resolve(chunk);
    },
    resolveNull() {
      if (!pending) throw new Error("no pending historyFetcher call to resolve");
      const resolve = pending;
      clear();
      resolve(null);
    },
    rejectFetch(err) {
      if (!pendingReject) throw new Error("no pending historyFetcher call to reject");
      const reject = pendingReject;
      clear();
      reject(err);
    },
    hasPending: () => pending !== null,
  };
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

export function aGhosttyTerminalGrpcLazyHistory() {
  const outputListeners: Array<(frame: GrpcFrame) => void> = [];
  const sentChunks: Uint8Array[] = [];
  const fetcher = aFetcherDouble();

  const stream: GrpcStream = {
    send(data: Uint8Array) {
      sentChunks.push(data);
    },
    onMessage(fn: (frame: GrpcFrame) => void) {
      outputListeners.push(fn);
    },
    close() {},
  };

  const waitForTerminal = () =>
    byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");

  const waitForListener = () =>
    cy.wrap(null, { timeout: 10000 }).should(() => {
      expect(outputListeners.length, "GrpcStream onMessage listener registered").to.be.greaterThan(0);
    });

  return {
    mount() {
      mount(
        <div style={{ height: 400, width: 800, position: "relative" }}>
          <UploadProgressProvider>
            <GhosttyTerminalGrpc
              sessionToken="lazy-history-token"
              sessionId="lazy-history-session"
              stream={stream}
              historyFetcher={fetcher.fetch}
            />
          </UploadProgressProvider>
        </div>,
      );
      return this;
    },

    /** Push a server frame to the terminal (initial replay or live tail). */
    pushFrame(frame: GrpcFrame) {
      waitForListener().then(() => {
        outputListeners.forEach((fn) => fn(frame));
      });
      return this;
    },

    /** Push a live-tail bytes frame (offsets zeroed). */
    pushOutput(bytes: Uint8Array) {
      waitForListener().then(() => {
        outputListeners.forEach((fn) => fn(aLiveFrame(bytes)));
      });
      return this;
    },

    /** Activate the "Load earlier output" affordance (first-time fill trigger). */
    activateLoadEarlier() {
      byTestId(TEST_IDS.loadEarlierHistory).click();
      return this;
    },

    /** Activate the "View history" affordance (instant swap after the first fill). */
    activateViewHistory() {
      byTestId(TEST_IDS.viewHistory).click();
      return this;
    },

    /** Activate the "Back to live" affordance on the page pane. */
    activateBackToLive() {
      byTestId(TEST_IDS.backToLive).click();
      return this;
    },

    /** Simulate a scroll-up-at-top gesture (wheel up while pinned to the bottom) on the live pane. */
    scrollUpAtTop() {
      byTestId(TEST_IDS.terminalLivePane).trigger("wheel", {
        deltaY: -120,
        clientX: 100,
        clientY: 50,
      });
      return this;
    },

    /** Simulate a wheel-down gesture on the live pane (deltaY > 0). Used to verify the mouse-tracking
     *  gate routes wheel-down to the TUI as SGR button 65 (not arrow-down). */
    scrollDownOnLive() {
      byTestId(TEST_IDS.terminalLivePane).trigger("wheel", {
        deltaY: 120,
        clientX: 100,
        clientY: 50,
      });
      return this;
    },

    /** Enter the alternate screen (DEC 1049) on the live terminal by writing the real escape sequence.
     *  ghostty-web processes it and reports isAlternateScreen() === true — no test-only hook required. */
    enterLiveAltScreen() {
      this.pushOutput(enc("\x1b[?1049h"));
      return this;
    },

    /** Exit the alternate screen (DEC 1049) on the live terminal. */
    exitLiveAltScreen() {
      this.pushOutput(enc("\x1b[?1049l"));
      return this;
    },

    /** Assert the bytes sent to the PTY (via stream.send) include the given substring. */
    expectLiveSentIncludes(substr: string) {
      cy.wrap(null, { timeout: 4000 }).should(() => {
        const text = new TextDecoder().decode(concatUint8(sentChunks));
        expect(text, "live PTY sent bytes").to.include(substr);
      });
      return this;
    },

    /** Assert the bytes sent to the PTY (via stream.send) do NOT include the given substring. */
    expectLiveDidNotSend(substr: string) {
      cy.wrap(null, { timeout: 4000 }).should(() => {
        const text = new TextDecoder().decode(concatUint8(sentChunks));
        expect(text, "live PTY sent bytes").to.not.include(substr);
      });
      return this;
    },

    /** Simulate a scroll-down-at-bottom gesture (wheel down while pinned) on the page pane. */
    scrollDownAtBottom() {
      byTestId(TEST_IDS.terminalPagePane).trigger("wheel", {
        deltaY: 120,
        clientX: 100,
        clientY: 50,
      });
      return this;
    },

    /**
     * Drive the page terminal's viewport up by `lines` via its imperative scrollLines handle
     * (negative = up into scrollback). Real wheel events don't reach ghostty-web reliably under
     * Cypress, so component tests exercise the "full control of what position is in the viewport"
     * API through this hook deterministically.
     */
    scrollPageUp(lines: number) {
      cy.window().then((win) => {
        const hook = (win as unknown as { __tddyPageScrollUp?: (n: number) => void })
          .__tddyPageScrollUp;
        expect(hook, "test-only page scrollUp hook registered").to.exist;
        hook!(lines);
      });
      return this;
    },

    /**
     * Drive the page terminal's viewport to an absolute line via scrollToLine (0 = top of
     * scrollback, scrollbackLength = bottom). The native "full control" viewport API.
     */
    scrollPageToLine(line: number) {
      cy.window().then((win) => {
        const hook = (win as unknown as { __tddyPageScrollToLine?: (n: number) => void })
          .__tddyPageScrollToLine;
        expect(hook, "test-only page scrollToLine hook registered").to.exist;
        hook!(line);
      });
      return this;
    },

    /**
     * Toggle mouse tracking (DEC 1006) on the live terminal. When tracking is on, the wheel is
     * reported to the TUI (SGR button 64/65) and does NOT scroll the viewport.
     */
    enableLiveMouseTracking(on = true) {
      cy.window().then((win) => {
        const hook = (win as unknown as { __tddyLiveMouseTracking?: (on: boolean) => void })
          .__tddyLiveMouseTracking;
        expect(hook, "test-only live mouse-tracking hook registered").to.exist;
        hook!(on);
      });
      return this;
    },

    /** Assert the LIVE terminal's mirrored viewportY equals the given value. */
    expectLiveViewportY(expected: number) {
      byTestId(TEST_IDS.terminalLiveViewportY).should(($el) => {
        expect(
          Number($el[0].textContent ?? "NaN"),
          "live terminal viewportY",
        ).to.equal(expected);
      });
      return this;
    },

    /** Assert the LIVE terminal's mirrored scrollback length equals the given value. */
    expectLiveScrollbackLength(expected: number) {
      byTestId(TEST_IDS.terminalLiveScrollbackLength).should(($el) => {
        expect(
          Number($el[0].textContent ?? "NaN"),
          "live terminal scrollback length",
        ).to.equal(expected);
      });
      return this;
    },

    /** Capture the LIVE terminal's scrollback length for later comparison (returns a Chainable). */
    captureLiveScrollbackLength(): Cypress.Chainable<number> {
      return byTestId(TEST_IDS.terminalLiveScrollbackLength).then(($el) =>
        Number($el[0].textContent ?? "NaN"),
      );
    },

    /** Capture the PAGE terminal's native Scrollbar {total, offset, len} for later comparison. */
    capturePageScrollbar(): Cypress.Chainable<{ total: number; offset: number; len: number }> {
      return byTestId(TEST_IDS.terminalPageScrollbar)
        .should(($el) => {
          const parts = ($el[0].textContent ?? "").split(",").map((s) => Number(s));
          expect(parts.length, "page scrollbar mirror parts").to.equal(3);
          expect(parts[0], "page scrollbar total").to.be.greaterThan(0);
        })
        .then(($el) => {
          const parts = ($el[0].textContent ?? "").split(",").map((s) => Number(s));
          return { total: parts[0] ?? 0, offset: parts[1] ?? 0, len: parts[2] ?? 0 };
        });
    },

    /**
     * Assert the PAGE terminal's native Scrollbar mirror carries {total, offset, len} with the
     * given values (the single source of truth for viewport position, same coordinate space as
     * scrollToLine).
     */
    expectPageScrollbar(expected: { total: number; offset: number; len: number }) {
      byTestId(TEST_IDS.terminalPageScrollbar).should(($el) => {
        const raw = ($el[0].textContent ?? "").split(",").map((s) => Number(s));
        expect(raw.length, "page scrollbar mirror parts").to.equal(3);
        expect(raw[0], "page scrollbar total").to.equal(expected.total);
        expect(raw[1], "page scrollbar offset").to.equal(expected.offset);
        expect(raw[2], "page scrollbar len").to.equal(expected.len);
      });
      return this;
    },

    /** Assert the LIVE terminal's native Scrollbar mirror carries {total, offset, len}. */
    expectLiveScrollbar(expected: { total: number; offset: number; len: number }) {
      byTestId(TEST_IDS.terminalLiveScrollbar).should(($el) => {
        const raw = ($el[0].textContent ?? "").split(",").map((s) => Number(s));
        expect(raw.length, "live scrollbar mirror parts").to.equal(3);
        expect(raw[0], "live scrollbar total").to.equal(expected.total);
        expect(raw[1], "live scrollbar offset").to.equal(expected.offset);
        expect(raw[2], "live scrollbar len").to.equal(expected.len);
      });
      return this;
    },

    /** Assert the page terminal's mirrored viewportY equals the given value. */
    expectPageViewportY(expected: number) {
      byTestId(TEST_IDS.terminalPageViewportY).should(($el) => {
        expect(
          Number($el[0].textContent ?? "NaN"),
          "page terminal viewportY",
        ).to.equal(expected);
      });
      return this;
    },

    /** Resolve the in-flight history fetch with a chunk. */
    resolveHistoryChunk(chunk: HistoryChunk) {
      cy.then(() => {
        fetcher.resolveChunk(chunk);
      });
      return this;
    },

    /** Resolve the in-flight history fetch with null (no older bytes). */
    resolveHistoryNull() {
      cy.then(() => {
        fetcher.resolveNull();
      });
      return this;
    },

    /** Reject the in-flight history fetch (simulates an RPC error, e.g. `getTerminalHistory` not_found). */
    rejectHistoryFetch(err: unknown) {
      cy.then(() => {
        fetcher.rejectFetch(err);
      });
      return this;
    },

    /** Wait until a historyFetcher call is awaiting resolution. */
    expectHistoryFetchPending(timeout = 4000) {
      cy.wrap(fetcher, { timeout }).should((f: FetcherDouble) => {
        expect(f.hasPending(), "historyFetcher call pending").to.be.true;
      });
      return this;
    },

    /** Assert the most recent historyFetcher call received the given from/until offsets. */
    expectHistoryFetchCalledWith(from: bigint, until: bigint) {
      cy.wrap(fetcher.calls).should((calls: FetchCall[]) => {
        const last = calls[calls.length - 1];
        expect(last, "historyFetcher called with from/until").to.deep.equal({ from, until });
      });
      return this;
    },

    /** Assert no further historyFetcher call has been made since the last check. */
    expectNoFurtherHistoryFetch() {
      cy.then(() => {
        const before = fetcher.calls.length;
        cy.wait(150, { log: false }).then(() => {
          expect(fetcher.calls.length, "no further historyFetcher call").to.equal(before);
        });
      });
      return this;
    },

    /** Assert no historyFetcher call has been started at all (the wheel gesture did NOT trigger a
     *  forward-fill). Stronger than expectNoFurtherHistoryFetch, which only guards against ADDITIONAL
     *  calls after an earlier one. */
    expectNoHistoryFetchStarted() {
      cy.wrap(fetcher.calls, { timeout: 1000 }).should(
        (calls: FetchCall[]) => {
          expect(calls.length, "no historyFetcher call started").to.equal(0);
        },
      );
      return this;
    },

    /** Assert the "Load earlier output" affordance is visible. */
    expectAffordanceVisible() {
      byTestId(TEST_IDS.loadEarlierHistory).should("exist").and("be.visible");
      return this;
    },

    /** Assert the "Load earlier output" affordance is absent. */
    expectAffordanceAbsent() {
      byTestId(TEST_IDS.loadEarlierHistory).should("not.exist");
      return this;
    },

    /** Assert the loading indicator is visible (background page terminal is being filled). */
    expectLoadingVisible() {
      byTestId(TEST_IDS.terminalHistoryLoading).should("exist").and("be.visible");
      return this;
    },

    /** Assert the loading indicator is absent. */
    expectLoadingAbsent() {
      byTestId(TEST_IDS.terminalHistoryLoading).should("not.exist");
      return this;
    },

    /** Assert the live pane is the foreground (visible, interactive). */
    expectLiveForeground() {
      byTestId(TEST_IDS.terminalLivePane).should("have.attr", "data-foreground", "true");
      byTestId(TEST_IDS.terminalPagePane).should("have.attr", "data-foreground", "false");
      return this;
    },

    /** Assert the page pane is the foreground (visible, interactive) and the live pane is hidden. */
    expectPageForeground() {
      byTestId(TEST_IDS.terminalPagePane).should("have.attr", "data-foreground", "true");
      byTestId(TEST_IDS.terminalLivePane).should("have.attr", "data-foreground", "false");
      return this;
    },

    /** Assert the "Back to live" affordance is visible on the page pane. */
    expectBackToLiveVisible() {
      byTestId(TEST_IDS.backToLive).should("exist").and("be.visible");
      return this;
    },

    /** Assert the "View history" affordance is visible on the live pane (after the first fill). */
    expectViewHistoryVisible() {
      byTestId(TEST_IDS.viewHistory).should("exist").and("be.visible");
      return this;
    },

    /**
     * Assert the LIVE terminal buffer contains the given texts in order. Reads the hidden
     * `terminal-buffer-text` mirror (the live terminal — scrollback 0, current screen only).
     */
    expectLiveBufferContainsInOrder(...texts: string[]) {
      byTestId("terminal-buffer-text").should(($el) => {
        const text = $el[0].textContent ?? "";
        let from = 0;
        for (const expected of texts) {
          const at = text.indexOf(expected, from);
          expect(at, `live buffer contains "${expected}" after the previous text`).to.be.greaterThan(-1);
          from = at + expected.length;
        }
      });
      return this;
    },

    /**
     * Assert the OLDER-history page terminal buffer contains the given texts in order. Reads the
     * hidden `terminal-older-buffer-text` mirror (the forward-filled page terminal).
     */
    expectOlderBufferContainsInOrder(...texts: string[]) {
      byTestId(TEST_IDS.terminalOlderBufferText).should(($el) => {
        const text = $el[0].textContent ?? "";
        let from = 0;
        for (const expected of texts) {
          const at = text.indexOf(expected, from);
          expect(at, `older buffer contains "${expected}" after the previous text`).to.be.greaterThan(-1);
          from = at + expected.length;
        }
      });
      return this;
    },

    /** Wait for the terminal element to exist (ready). */
    expectReady() {
      waitForTerminal();
      return this;
    },
  };
}
