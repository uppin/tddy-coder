/**
 * Fluent component driver for `GhosttyTerminalSession`'s chrome.
 *
 * Centralises:
 *  - the feed the terminal is mounted on (a stand-in a spec can push bytes into and read sends from)
 *  - `win.Element.prototype.requestFullscreen` stub setup
 *  - `win.confirm` stub setup
 *  - mounting inside the required positioned container
 *
 * The terminal no longer connects anything, so there is no url, no token and no room here: a spec
 * that wants output delivers it, and one that only exercises chrome need not think about bytes at
 * all.
 *
 * `sent`, `deliver()` and `endSession()` drive the driver's own feed and throw if a spec supplied
 * its own `feed` — binding them to a feed the component was never handed makes assertions pass
 * against an array nothing writes to.
 *
 * Usage:
 *
 *   aGhosttyTerminalSession()
 *     .withTerminate()
 *     .stubRequestFullscreen()
 *     .mount()
 *     .openStatusMenu()
 *     .clickDisconnect()
 *     .expectDisconnectCalled();
 */

import React from "react";
import { mount } from "cypress/react";
import type { GhosttyTerminalSessionProps } from "../../../src/components/GhosttyTerminalSession";
import { GhosttyTerminalSession } from "../../../src/components/GhosttyTerminalSession";
import type { TerminalFeed, TerminalFrame, TerminalStream } from "../../../src/rpc/connections/terminal";
import type { ToolShortcutDef } from "../../../src/lib/toolShortcuts";
import { byTestId, TEST_IDS } from "../testIds";

// ---------------------------------------------------------------------------
// The feed the terminal is mounted on
// ---------------------------------------------------------------------------

export interface ControllableFeed {
  /** What the component is handed. */
  feed: TerminalFeed;
  /** Deliver one live-tail frame of output. */
  deliver: (text: string) => void;
  /** Whether the terminal has registered its listener — a frame delivered before it is dropped. */
  isListening: () => boolean;
  /** Everything the terminal has written back. */
  sent: Uint8Array[];
  /** Settle the feed's `ended` — the far end is gone. */
  endSession: () => void;
}

/** A feed a spec drives by hand: bytes in, bytes out, and an end it can trigger. */
export function aControllableFeed(
  options: { history?: TerminalFeed["history"] } = {},
): ControllableFeed {
  const listeners: Array<(frame: TerminalFrame) => void> = [];
  const sent: Uint8Array[] = [];
  let endSession = () => {};
  const ended = new Promise<void>((resolve) => {
    endSession = resolve;
  });

  const stream: TerminalStream = {
    send: (data) => sent.push(data),
    onMessage: (fn) => listeners.push(fn),
    close: () => {},
  };

  return {
    feed: {
      stream,
      ended,
      ...(options.history ? { history: options.history } : {}),
    },
    deliver: (text: string) => {
      const frame: TerminalFrame = {
        data: new TextEncoder().encode(text),
        endOffset: 0n,
        atOldest: false,
      };
      for (const fn of listeners) fn(frame);
    },
    isListening: () => listeners.length > 0,
    sent,
    endSession: () => endSession(),
  };
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

export interface GhosttyTerminalSessionDriverOptions {
  /**
   * The feed to mount on. Defaults to a fresh controllable one with no history.
   *
   * A spec that brings its own owns the bytes on both sides of it, so `sent`, `deliver()` and
   * `endSession()` — which drive the driver's own feed — refuse to answer for it.
   */
  feed?: TerminalFeed;
  /**
   * What a connection that can replay older output serves, for the driver's own feed. Supplying
   * one is what makes the terminal offer scrollback.
   */
  history?: TerminalFeed["history"];
  /** What the caller says about the connection carrying the feed. Omitted by default. */
  connectionStatus?: GhosttyTerminalSessionProps["connectionStatus"];
  /** Whether to show the mobile keyboard affordance. */
  showMobileKeyboard?: boolean;
  /** Whether to prevent focus on tap. */
  preventFocusOnTap?: boolean;
  /** Connection overlay options (enables chrome). If omitted, no overlay. */
  connectionOverlay?: GhosttyTerminalSessionProps["connectionOverlay"];
  /** Container height (default 400). */
  containerHeight?: number;
  /** Container width (default unset). */
  containerWidth?: number;
  /** Mobile shortcut presets to pass to the component. */
  mobileShortcuts?: ToolShortcutDef[];
}

export function aGhosttyTerminalSession(opts: GhosttyTerminalSessionDriverOptions = {}) {
  const onDisconnect = cy.stub().as("onDisconnect");
  const onTerminate = cy.stub().as("onTerminate");

  const containerHeight = opts.containerHeight ?? 400;
  const controllable = aControllableFeed({ history: opts.history });
  const feed = opts.feed ?? controllable.feed;

  /**
   * Guard the three members that drive the driver's own feed.
   *
   * Silently binding them to a feed the component was never handed is how a spec ends up asserting
   * on an array nothing writes to, and passing.
   */
  const requireOwnFeed = (member: string) => {
    if (opts.feed) {
      throw new Error(
        `${member} drives this driver's own feed, but the spec supplied \`feed\`. ` +
          "Drive that feed directly, or drop the `feed` option and use the driver's.",
      );
    }
  };

  let connectionOverlay: GhosttyTerminalSessionProps["connectionOverlay"] = opts.connectionOverlay;

  const driver = {
    /** The bytes the terminal has written back to the driver's own feed. */
    get sent(): Uint8Array[] {
      requireOwnFeed("`sent`");
      return controllable.sent;
    },

    /**
     * Deliver output on the driver's own feed, once the terminal is listening.
     *
     * A frame delivered before the mounting effect has registered the listener is dropped and never
     * retried, so this settles on the registration first rather than racing it.
     */
    deliver(text: string) {
      requireOwnFeed("`deliver()`");
      cy.wrap(null, { log: false, timeout: 10000 })
        .should(() => {
          expect(controllable.isListening(), "terminal registered its feed listener").to.be.true;
        })
        .then(() => controllable.deliver(text));
      return driver;
    },

    /** End the session on the driver's own feed — the far end is gone. */
    endSession() {
      requireOwnFeed("`endSession()`");
      cy.then(() => controllable.endSession());
      return driver;
    },

    /**
     * Add connection overlay with Disconnect handler (required for chrome tests).
     */
    withDisconnect(buildId?: string) {
      connectionOverlay = { onDisconnect, buildId };
      return driver;
    },

    /**
     * Add connection overlay with both Disconnect and Terminate handlers.
     */
    withTerminate(buildId?: string) {
      connectionOverlay = { onDisconnect, onTerminate, buildId };
      return driver;
    },

    /**
     * Stub `win.Element.prototype.requestFullscreen`.
     * Must be called AFTER mount (Cypress stubs window objects post-mount).
     */
    stubRequestFullscreen() {
      cy.window().then((win) => {
        cy.stub(win.Element.prototype, "requestFullscreen")
          .as("requestFullscreenStub")
          .resolves();
      });
      return driver;
    },

    /**
     * Stub `win.confirm` to return the given value.
     * Call BEFORE mount (confirm may be invoked during mount lifecycle).
     */
    stubConfirm(returns: boolean) {
      cy.window().then((win) => {
        cy.stub(win, "confirm").returns(returns).as("confirmStub");
      });
      return driver;
    },

    /** Mount the component inside a positioned container of the configured dimensions. */
    mount() {
      const style: React.CSSProperties = {
        height: containerHeight,
        position: "relative",
      };
      if (opts.containerWidth !== undefined) {
        style.width = opts.containerWidth;
      }

      mount(
        <div style={style}>
          <GhosttyTerminalSession
            feed={feed}
            connectionStatus={opts.connectionStatus}
            showMobileKeyboard={opts.showMobileKeyboard}
            preventFocusOnTap={opts.preventFocusOnTap}
            connectionOverlay={connectionOverlay}
            mobileShortcuts={opts.mobileShortcuts}
          />
        </div>,
      );
      return driver;
    },

    // ---------------------------------------------------------------------------
    // Queries
    // ---------------------------------------------------------------------------

    /** The connection status dot. */
    statusDot: (options?: Parameters<typeof cy.get>[1]) =>
      byTestId(TEST_IDS.connectionStatusDot, { timeout: 10000, ...options }),

    /** The raw connection status readout. */
    livekitStatus: () => byTestId(TEST_IDS.livekitStatus),

    /** The terminal fullscreen button. */
    fullscreenButton: (options?: Parameters<typeof cy.get>[1]) =>
      byTestId(TEST_IDS.terminalFullscreenButton, { timeout: 5000, ...options }),

    /** The mobile keyboard overlay button. */
    mobileKeyboardButton: (options?: Parameters<typeof cy.get>[1]) =>
      byTestId(TEST_IDS.mobileKeyboardButton, { timeout: 10000, ...options }),

    /** The terminal connection status bar (wraps the chrome). */
    statusBar: (options?: Parameters<typeof cy.get>[1]) =>
      byTestId(TEST_IDS.terminalConnectionStatusBar, { timeout: 20000, ...options }),

    /** The Ghostty terminal canvas container. */
    terminal: (options?: Parameters<typeof cy.get>[1]) =>
      byTestId(TEST_IDS.ghosttyTerminal, { timeout: 20000, ...options }),

    /** The readable mirror of what the live terminal has painted (the canvas itself is WebGL). */
    bufferText: (options?: Parameters<typeof cy.get>[1]) =>
      byTestId(TEST_IDS.terminalBufferText, { timeout: 20000, ...options }),

    /** The readable mirror of the older-history page terminal. */
    olderBufferText: (options?: Parameters<typeof cy.get>[1]) =>
      byTestId(TEST_IDS.terminalOlderBufferText, { timeout: 20000, ...options }),

    /** The "Load earlier output" scrollback affordance. */
    loadEarlierButton: (options?: Parameters<typeof cy.get>[1]) =>
      byTestId(TEST_IDS.loadEarlierHistory, { timeout: 20000, ...options }),

    /** The cover shown once the far end has ended the session. */
    coderUnavailableCover: (options?: Parameters<typeof cy.get>[1]) =>
      byTestId(TEST_IDS.terminalCoderUnavailable, { timeout: 10000, ...options }),

    /** Build ID label. */
    buildId: () => byTestId(TEST_IDS.buildId),

    /** "Disconnect" item in the open status menu. */
    disconnectMenuItem: (options?: Parameters<typeof cy.get>[1]) =>
      byTestId(TEST_IDS.connectionMenuDisconnect, { timeout: 3000, ...options }),

    /** "Terminate" item in the open status menu. */
    terminateMenuItem: (options?: Parameters<typeof cy.get>[1]) =>
      byTestId(TEST_IDS.connectionMenuTerminate, { timeout: 3000, ...options }),

    // ---------------------------------------------------------------------------
    // Interactions
    // ---------------------------------------------------------------------------

    /** Settle once the terminal canvas is mounted and ready to be driven. */
    expectReady() {
      driver.terminal().should("exist");
      return driver;
    },

    /** Type at the terminal, as an operator at a keyboard would. */
    type(text: string) {
      driver.terminal().should("exist").focus().type(text);
      return driver;
    },

    /** Type on the mobile keyboard affordance's hidden input, as a soft keyboard does. */
    typeOnMobileKeyboard(text: string) {
      driver.mobileKeyboardButton().within(() => {
        cy.get("input").focus().type(text);
      });
      return driver;
    },

    /** Activate the "Load earlier output" scrollback affordance. */
    loadEarlierOutput() {
      driver.loadEarlierButton().should("be.visible").click();
      return driver;
    },

    /** Click the status dot to open the connection menu. */
    openStatusMenu() {
      driver.statusDot().should("be.visible").click();
      return driver;
    },

    /** Assert that the connection menu is open (disconnect item is visible). */
    expectMenuOpen() {
      driver.disconnectMenuItem().should("be.visible");
      return driver;
    },

    /** Click "Disconnect" in the open connection menu. */
    clickDisconnect() {
      // force:true bypasses coordinate-based mousedown interception from the canvas
      driver.disconnectMenuItem().should("be.visible").click({ force: true });
      return driver;
    },

    /** Click "Terminate" in the open connection menu. */
    clickTerminate() {
      driver.terminateMenuItem().should("be.visible").click({ force: true });
      return driver;
    },

    /** Click the fullscreen button. */
    clickFullscreen() {
      driver.fullscreenButton().should("be.visible").click();
      return driver;
    },

    // ---------------------------------------------------------------------------
    // Assertions
    // ---------------------------------------------------------------------------

    /** Assert the terminal has painted the given text. */
    expectTerminalShows(text: string) {
      driver.bufferText().should("contain.text", text);
      return driver;
    },

    /** Assert the older-history page terminal holds the given text. */
    expectEarlierOutputShows(text: string) {
      driver.olderBufferText().should("contain.text", text);
      return driver;
    },

    /**
     * Assert one of the chunks the terminal wrote back to the feed is exactly this text.
     *
     * Chunk by chunk, not the concatenation: the terminal also emits a resize OSC
     * (`\x1b]resize;80;24\x07`), and searching the joined bytes for "s" finds that instead.
     */
    expectSentToFeed(text: string) {
      cy.wrap(null, { log: false, timeout: 4000 }).should(() => {
        const chunks = driver.sent.map((c) => new TextDecoder().decode(c));
        expect(chunks, "chunks written back to the feed").to.include(text);
      });
      return driver;
    },

    /** Assert no chunk the terminal wrote back to the feed is this text. */
    expectNotSentToFeed(text: string) {
      cy.wrap(null, { log: false }).should(() => {
        const chunks = driver.sent.map((c) => new TextDecoder().decode(c));
        expect(chunks, "chunks written back to the feed").to.not.include(text);
      });
      return driver;
    },

    /** Assert the terminal offers scrollback — the connection can replay older output. */
    expectScrollbackOffered() {
      driver.loadEarlierButton().should("be.visible");
      return driver;
    },

    /** Assert the terminal offers no scrollback — the connection cannot replay. */
    expectNoScrollbackOffered() {
      byTestId(TEST_IDS.loadEarlierHistory).should("not.exist");
      return driver;
    },

    /** Assert the older-history page is the foreground pane — the user is browsing scrollback. */
    expectViewingEarlierOutput() {
      byTestId(TEST_IDS.terminalPagePane).should("have.attr", "data-foreground", "true");
      byTestId(TEST_IDS.terminalLivePane).should("have.attr", "data-foreground", "false");
      return driver;
    },

    /** Assert the terminal says the coder is gone. */
    expectCoderUnavailable() {
      driver.coderUnavailableCover().should("be.visible");
      return driver;
    },

    /** Assert the raw connection status readout states this status, visibly. */
    expectStatusReadout(status: string) {
      driver.livekitStatus().should("be.visible").and("have.text", status);
      return driver;
    },

    expectDisconnectCalled() {
      cy.get("@onDisconnect").should("have.been.calledOnce");
      return driver;
    },

    expectTerminateCalled() {
      cy.get("@onTerminate").should("have.been.calledOnce");
      return driver;
    },

    expectTerminateNotCalled() {
      cy.get("@onTerminate").should("not.have.been.called");
      return driver;
    },

    expectRequestFullscreenCalled() {
      cy.get("@requestFullscreenStub").should("have.been.calledOnce");
      return driver;
    },

    expectStatusDotVisible() {
      driver.statusDot().should("be.visible").and("have.attr", "data-connection-status");
      return driver;
    },

    expectLivekitStatusHidden() {
      driver.livekitStatus().should("not.be.visible");
      return driver;
    },

    expectMobileKeyboardExists() {
      driver.mobileKeyboardButton().should("exist");
      return driver;
    },

    expectMobileKeyboardNotExists() {
      byTestId(TEST_IDS.mobileKeyboardButton).should("not.exist");
      return driver;
    },

    /** The shortcut drawer (floating panel). */
    shortcutDrawer: (options?: Parameters<typeof cy.get>[1]) =>
      byTestId(TEST_IDS.shortcutDrawer, options),

    expectShortcutDrawerExists() {
      driver.shortcutDrawer().should("exist");
      return driver;
    },

    expectShortcutDrawerNotExists() {
      byTestId(TEST_IDS.shortcutDrawer).should("not.exist");
      return driver;
    },
  };

  return driver;
}
