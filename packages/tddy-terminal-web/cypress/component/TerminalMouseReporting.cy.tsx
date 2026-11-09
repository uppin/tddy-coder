/**
 * A program that tracks the mouse receives exactly one report per click and per wheel step.
 *
 * xterm.js reports mouse and wheel input to a tracking program itself. The web terminal used to do
 * that by hand for xterm.js; if both paths run, every click and wheel step reaches the program
 * twice. These specs drive the terminal component the way a browser does — events on xterm's
 * screen element — and assert on the SGR reports the terminal sent as input.
 *
 * Docs: packages/tddy-terminal-web/docs/xterm-engine.md
 */

import React from "react";
import { TerminalView, type TerminalViewHandle } from "../../src/TerminalView";
import { byTestId, TEST_IDS } from "../support/testIds";

const ENABLE_CLICK_TRACKING_SGR = "\x1b[?1000h\x1b[?1006h";
const SGR_REPORT = /\x1b\[<(\d+);(\d+);(\d+)([Mm])/g;

type SgrReport = { button: number; kind: "press" | "release" };

function sgrReportsIn(input: string[]): SgrReport[] {
  return [...input.join("").matchAll(SGR_REPORT)].map(([, button, , , kind]) => ({
    button: Number(button),
    kind: kind === "M" ? "press" : "release",
  }));
}

function aTerminalRunningAMouseTrackingProgram() {
  const terminal = React.createRef<TerminalViewHandle>();
  const input: string[] = [];
  cy.mount(
    <div style={{ width: 800, height: 400 }}>
      <TerminalView ref={terminal} onData={(data) => input.push(data)} />
    </div>,
  );
  byTestId(TEST_IDS.terminal, { timeout: 10000 }).should("have.attr", "data-terminal-ready", "true");
  cy.then(() => terminal.current?.write(ENABLE_CLICK_TRACKING_SGR));
  cy.wrap(terminal).should((t) => expect(t.current?.hasMouseTracking?.(), "mouse tracking on").to.equal(true));

  const screen = () => byTestId(TEST_IDS.terminal).find(".xterm-screen");
  return {
    click() {
      screen().trigger("mousedown", 40, 20, { button: 0, buttons: 1 });
      screen().trigger("mouseup", 40, 20, { button: 0, buttons: 0 });
      return this;
    },
    wheelUpOneStep() {
      screen().trigger("wheel", 40, 20, { deltaY: -1, deltaMode: WheelEvent.DOM_DELTA_LINE });
      return this;
    },
    wheelUpOnePixelNotch() {
      screen().trigger("wheel", 40, 20, { deltaY: -100, deltaMode: WheelEvent.DOM_DELTA_PIXEL });
      return this;
    },
    expectReports(expected: SgrReport[]) {
      cy.wrap(input, { timeout: 4000 }).should((received) => {
        expect(sgrReportsIn(received), `raw input ${JSON.stringify(received)}`).to.deep.equal(expected);
      });
      return this;
    },
  };
}

describe("Mouse reporting to a program that tracks the mouse", () => {
  it("sends one press and one release for a click", () => {
    // Given
    const terminal = aTerminalRunningAMouseTrackingProgram();

    // When
    terminal.click();

    // Then
    terminal.expectReports([
      { button: 0, kind: "press" },
      { button: 0, kind: "release" },
    ]);
  });

  it("sends one wheel-up report for one wheel step", () => {
    // Given
    const terminal = aTerminalRunningAMouseTrackingProgram();

    // When
    terminal.wheelUpOneStep();

    // Then
    terminal.expectReports([{ button: 64, kind: "press" }]);
  });

  it("sends one wheel-up report for one pixel-mode wheel notch", () => {
    // Given
    const terminal = aTerminalRunningAMouseTrackingProgram();

    // When — one notch of a typical mouse wheel, reported in pixels
    terminal.wheelUpOnePixelNotch();

    // Then
    terminal.expectReports([{ button: 64, kind: "press" }]);
  });
});
