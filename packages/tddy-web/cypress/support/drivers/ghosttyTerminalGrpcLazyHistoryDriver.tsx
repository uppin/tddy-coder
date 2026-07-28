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
  /** True when a fetch is awaiting resolution. */
  hasPending: () => boolean;
}

function aFetcherDouble(): FetcherDouble {
  const calls: FetchCall[] = [];
  let pending: ((chunk: HistoryChunk | null) => void) | null = null;
  const fetch = (fromOffset: bigint, untilOffset: bigint): Promise<HistoryChunk | null> => {
    calls.push({ from: fromOffset, until: untilOffset });
    return new Promise<HistoryChunk | null>((resolve) => {
      pending = resolve;
    });
  };
  return {
    fetch,
    calls,
    resolveChunk(chunk) {
      if (!pending) throw new Error("no pending historyFetcher call to resolve");
      const resolve = pending;
      pending = null;
      resolve(chunk);
    },
    resolveNull() {
      if (!pending) throw new Error("no pending historyFetcher call to resolve");
      const resolve = pending;
      pending = null;
      resolve(null);
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
     * `terminal-buffer-text` mirror (the live terminal — scrollback stays 0).
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
