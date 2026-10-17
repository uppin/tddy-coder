/**
 * Fluent component driver: a real `GhosttyTerminalSession` wired to the wheel-reporting test TUI.
 *
 * The TUI's own output (mode setup included) arrives through the terminal feed exactly as a PTY's
 * would, and whatever the terminal sends back is handed to the TUI as its stdin — so the specs
 * assert on what the application *received*, not on what the component says it sent.
 */

import React from "react";
import { mount } from "cypress/react";
import { GhosttyTerminalSession } from "../../../src/components/GhosttyTerminalSession";
import type { TerminalFeed, TerminalStream } from "../../../src/rpc/connections/terminal";
import { UploadProgressProvider } from "../../../src/rpc/uploadProgress";
import { WheelTuiApp, type WheelTuiOptions } from "../../../scripts/wheel-tui/wheelTuiApp";
import { byTestId, TEST_IDS } from "../testIds";

const enc = (s: string): Uint8Array => new TextEncoder().encode(s);
const dec = (b: Uint8Array): string => new TextDecoder().decode(b);

interface Frame {
  data: Uint8Array;
  endOffset: bigint;
  atOldest: boolean;
}

export interface WheelTuiSessionOptions extends WheelTuiOptions {
  /** Whether the connection can serve scrollback (`feed.history`); local feeds may not. */
  canServeHistory: boolean;
  /** Deliver the TUI's launch output as the attach replay (a TUI already running) rather than live. */
  launchedBeforeAttach: boolean;
}

export function aWheelTuiSession(options: WheelTuiSessionOptions) {
  const app = new WheelTuiApp(options);
  const listeners: Array<(frame: Frame) => void> = [];
  const receivedByApp: string[] = [];

  const deliver = (data: string, endOffset = 0n) =>
    cy.wrap(null, { timeout: 10000 }).should(() => {
      expect(listeners.length, "terminal feed listener registered").to.be.greaterThan(0);
    }).then(() => listeners.forEach((fn) => fn({ data: enc(data), endOffset, atOldest: false })));

  const stream: TerminalStream = {
    send(bytes: Uint8Array) {
      const input = dec(bytes);
      receivedByApp.push(input);
      const repaint = app.receive(input);
      setTimeout(() => listeners.forEach((fn) => fn({ data: enc(repaint), endOffset: 0n, atOldest: false })), 0);
    },
    onMessage(fn: (frame: Frame) => void) {
      listeners.push(fn);
    },
    close() {},
  };
  const feed: TerminalFeed = options.canServeHistory
    ? { stream, history: async () => null }
    : { stream };

  return {
    mount() {
      mount(
        <div style={{ height: 400, width: 800, position: "relative" }}>
          <UploadProgressProvider>
            <GhosttyTerminalSession feed={feed} sessionToken="wheel-tui-token" sessionId="wheel-tui-session" />
          </UploadProgressProvider>
        </div>,
      );
      byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("exist");
      return this;
    },

    /** The application starts and paints its first screen. */
    launchTui() {
      const launch = app.start();
      if (options.launchedBeforeAttach) deliver(launch, BigInt(launch.length));
      else deliver(launch);
      return this;
    },

    /** One wheel notch over the live pane, as the browser reports it (on the canvas). */
    wheelUp() {
      byTestId(TEST_IDS.terminalLivePane)
        .find("canvas")
        .trigger("wheel", { deltaY: -120, clientX: 100, clientY: 50 });
      return this;
    },

    wheelDown() {
      byTestId(TEST_IDS.terminalLivePane)
        .find("canvas")
        .trigger("wheel", { deltaY: 120, clientX: 100, clientY: 50 });
      return this;
    },

    /** The TUI counted exactly these wheel reports and no wheel-emulating arrow keys. */
    expectTuiReceivedWheel(expected: { up: number; down: number }) {
      cy.wrap(null, { timeout: 4000 }).should(() => {
        expect(
          { wheelUp: app.wheelUp, wheelDown: app.wheelDown, arrowUp: app.arrowUp, arrowDown: app.arrowDown },
          `what the TUI received (raw input: ${JSON.stringify(receivedByApp)})`,
        ).to.deep.equal({ wheelUp: expected.up, wheelDown: expected.down, arrowUp: 0, arrowDown: 0 });
      });
      return this;
    },
  };
}
