/**
 * Fluent component driver for GhosttyTerminalLiveKit.
 *
 * Centralises:
 *  - the default `getToken` stub used across tests
 *  - `win.Element.prototype.requestFullscreen` stub setup
 *  - `win.confirm` stub setup
 *  - mounting inside the required positioned container
 *
 * Usage:
 *
 *   aGhosttyTerminalLiveKit()
 *     .withTerminate()
 *     .stubRequestFullscreen()
 *     .mount()
 *     .openStatusMenu()
 *     .clickDisconnect()
 *     .expectDisconnectCalled();
 */

import React from "react";
import { mount } from "cypress/react";
import type { GhosttyTerminalLiveKitProps } from "../../../src/components/GhosttyTerminalLiveKit";
import { GhosttyTerminalLiveKit } from "../../../src/components/GhosttyTerminalLiveKit";
import { byTestId, TEST_IDS } from "../testIds";

// ---------------------------------------------------------------------------
// Canonical test token factory — duplicated in tests; lives here once.
// ---------------------------------------------------------------------------

export const defaultGetToken = () =>
  Promise.resolve({ token: "fake-token", ttlSeconds: BigInt(600) });

// ---------------------------------------------------------------------------
// Driver
// ---------------------------------------------------------------------------

export interface GhosttyTerminalLiveKitDriverOptions {
  /** LiveKit server URL (defaults to non-resolving local addr for unit tests). */
  url?: string;
  /** Initial token. */
  token?: string;
  /** Token refresh factory. Defaults to `defaultGetToken`. */
  getToken?: GhosttyTerminalLiveKitProps["getToken"];
  /** TTL in seconds. */
  ttlSeconds?: bigint;
  /** Whether to show the mobile keyboard overlay. */
  showMobileKeyboard?: boolean;
  /** Whether to prevent focus on tap. */
  preventFocusOnTap?: boolean;
  /** Connection overlay options (enables chrome). If omitted, no overlay. */
  connectionOverlay?: GhosttyTerminalLiveKitProps["connectionOverlay"];
  /** Container height (default 400). */
  containerHeight?: number;
  /** Container width (default unset). */
  containerWidth?: number;
}

export function aGhosttyTerminalLiveKit(
  opts: GhosttyTerminalLiveKitDriverOptions = {},
) {
  const onDisconnect = cy.stub().as("onDisconnect");
  const onTerminate = cy.stub().as("onTerminate");

  const url = opts.url ?? "ws://localhost:9999";
  const token = opts.token ?? "fake-token";
  const getToken = opts.getToken ?? defaultGetToken;
  const ttlSeconds = opts.ttlSeconds ?? BigInt(600);
  const containerHeight = opts.containerHeight ?? 400;

  let connectionOverlay: GhosttyTerminalLiveKitProps["connectionOverlay"] =
    opts.connectionOverlay;

  const driver = {
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
          <GhosttyTerminalLiveKit
            url={url}
            token={token}
            getToken={getToken}
            ttlSeconds={ttlSeconds}
            showMobileKeyboard={opts.showMobileKeyboard}
            preventFocusOnTap={opts.preventFocusOnTap}
            connectionOverlay={connectionOverlay}
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

    /** The LiveKit status text element. */
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
      driver.statusDot().should("exist").click();
      return driver;
    },

    /** Click "Disconnect" in the open connection menu. */
    clickDisconnect() {
      driver.disconnectMenuItem().should("be.visible").click();
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
  };

  return driver;
}
