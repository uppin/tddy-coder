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
  /** Everything the terminal has written back. */
  sent: Uint8Array[];
  /** Settle the feed's `ended` — the far end is gone. */
  endSession: () => void;
}

/** A feed a spec drives by hand: bytes in, bytes out, and an end it can trigger. */
export function aControllableFeed(options: { withHistory?: boolean } = {}): ControllableFeed {
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
      ...(options.withHistory ? { history: async () => null } : {}),
    },
    deliver: (text: string) => {
      const frame: TerminalFrame = {
        data: new TextEncoder().encode(text),
        endOffset: 0n,
        atOldest: false,
      };
      for (const fn of listeners) fn(frame);
    },
    sent,
    endSession: () => endSession(),
  };
}

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

export interface GhosttyTerminalSessionDriverOptions {
  /** The feed to mount on. Defaults to a fresh controllable one with no history. */
  feed?: TerminalFeed;
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
  const controllable = aControllableFeed();
  const feed = opts.feed ?? controllable.feed;

  let connectionOverlay: GhosttyTerminalSessionProps["connectionOverlay"] = opts.connectionOverlay;

  const driver = {
    /** The bytes the terminal has written back to its feed (only for the default feed). */
    sent: controllable.sent,

    /** Deliver output on the default feed. */
    deliver(text: string) {
      controllable.deliver(text);
      return driver;
    },

    /** End the session on the default feed — the far end is gone. */
    endSession() {
      controllable.endSession();
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
