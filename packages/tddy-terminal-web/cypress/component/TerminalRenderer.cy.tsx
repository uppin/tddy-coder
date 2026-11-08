/**
 * The web terminal renders with WebGL where the browser offers it and with the DOM renderer
 * otherwise, and says which on the terminal element (`data-terminal-renderer`).
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-08-xterm-terminal-engine.md (acceptance criterion 5)
 */

import React from "react";
import { GhosttyTerminal } from "../../src/GhosttyTerminal";
import { byTestId, TEST_IDS } from "../support/testIds";

function mountTerminal() {
  cy.mount(
    <div style={{ width: 800, height: 400 }}>
      <GhosttyTerminal />
    </div>,
  );
  byTestId(TEST_IDS.ghosttyTerminal, { timeout: 10000 }).should("have.attr", "data-terminal-ready", "true");
}

function rendererAttribute() {
  return byTestId(TEST_IDS.ghosttyTerminal).find("[data-terminal-renderer]");
}

/** Makes every WebGL context request in the page fail, as on a browser without WebGL. */
function withoutWebGl() {
  cy.window().then((win) => {
    const original = win.HTMLCanvasElement.prototype.getContext;
    cy.stub(win.HTMLCanvasElement.prototype, "getContext").callsFake(function (
      this: HTMLCanvasElement,
      type: string,
      ...rest: unknown[]
    ) {
      if (type.startsWith("webgl")) return null;
      return (original as (...args: unknown[]) => unknown).call(this, type, ...rest);
    });
  });
}

describe("Terminal renderer", () => {
  it("names the renderer it uses: WebGL or DOM", () => {
    // Given / When
    mountTerminal();

    // Then
    rendererAttribute()
      .invoke("attr", "data-terminal-renderer")
      .should("be.oneOf", ["webgl", "dom"]);
  });

  it("renders with the DOM renderer when WebGL is unavailable", () => {
    // Given
    withoutWebGl();

    // When
    mountTerminal();

    // Then
    rendererAttribute().should("have.attr", "data-terminal-renderer", "dom");
  });
});
