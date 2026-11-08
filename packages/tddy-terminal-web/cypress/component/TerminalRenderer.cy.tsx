/**
 * The web terminal renders with WebGL where the browser offers it and with the DOM renderer
 * otherwise, and says which on the terminal element (`data-terminal-renderer`).
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-08-xterm-terminal-engine.md (acceptance criterion 5)
 */

import React from "react";
import { GhosttyTerminal } from "../../src/GhosttyTerminal";
import { createXtermEngine, type WebglRendererAddon } from "../../src/engine/xtermEngine";
import type { TerminalEngine } from "../../src/engine/TerminalEngine";
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

  describe("when the WebGL context is lost", () => {
    let opened: TerminalEngine | null = null;
    afterEach(() => {
      opened?.dispose();
      opened = null;
    });

    it("switches to the DOM renderer and releases the WebGL one", () => {
      // Given — an engine rendering with a WebGL renderer (a stand-in: headless browsers have no GPU)
      const gpu = aWebglRendererThatCanLoseItsContext();
      anOpenXtermEngineRenderingWith(gpu).then((engine) => {
        opened = engine;
        expectEngineRenderer(engine, "webgl");

        // When
        cy.then(() => gpu.loseContext());

        // Then
        expectEngineRenderer(engine, "dom");
        cy.wrap(gpu).its("disposed").should("equal", true);
      });
    });
  });
});

type LosableWebglRenderer = WebglRendererAddon & { disposed: boolean; loseContext(): void };

function aWebglRendererThatCanLoseItsContext(): LosableWebglRenderer {
  const listeners: Array<() => void> = [];
  return {
    disposed: false,
    activate() {},
    dispose() {
      this.disposed = true;
    },
    onContextLoss(listener) {
      listeners.push(listener);
      return { dispose: () => void listeners.splice(listeners.indexOf(listener), 1) };
    },
    loseContext() {
      for (const listener of [...listeners]) listener();
    },
  };
}

function anOpenXtermEngineRenderingWith(renderer: WebglRendererAddon): Cypress.Chainable<TerminalEngine> {
  return cy.document().then((doc) => {
    const container = doc.createElement("div");
    container.style.width = "800px";
    container.style.height = "240px";
    doc.body.appendChild(container);
    const engine = createXtermEngine({ cols: 80, rows: 10, fontSize: 14 }, () => renderer);
    return cy.wrap(engine.open(container), { log: false }).then(() => engine);
  });
}

function expectEngineRenderer(engine: TerminalEngine, renderer: "webgl" | "dom"): void {
  cy.wrap(engine, { log: false }).should((e) => {
    const named = e.screenElement?.closest("[data-terminal-renderer]");
    expect(named?.getAttribute("data-terminal-renderer"), "data-terminal-renderer").to.equal(renderer);
  });
}
