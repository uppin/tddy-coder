/**
 * Wide characters take two cells, as the TUI that drew them laid them out.
 *
 * A TUI places text after an emoji or a CJK character assuming it is two columns wide (Unicode 11
 * widths). If the terminal draws it one column wide, everything after it on the line shifts left
 * and box borders break. Each test writes a wide character, moves the cursor to column 3
 * (1-based), and writes a marker: with a two-cell glyph the marker lands right after it, with a
 * one-cell glyph a blank cell separates them.
 *
 * Docs: packages/tddy-terminal-web/docs/xterm-engine.md
 */

import { xtermEngine } from "../../src/engine/xtermEngine";
import type { TerminalEngine } from "../../src/engine/TerminalEngine";

const MOVE_TO_COLUMN_3 = "\x1b[3G";

function anOpenXtermEngine(): Cypress.Chainable<TerminalEngine> {
  return cy.document().then((doc) => {
    const container = doc.createElement("div");
    container.style.width = "800px";
    container.style.height = "240px";
    doc.body.appendChild(container);
    const engine = xtermEngine({ cols: 80, rows: 10, fontSize: 14, scrollback: 100 });
    return cy.wrap(engine.open(container), { log: false }).then(() => engine);
  });
}

function expectFirstLine(engine: TerminalEngine, text: string): void {
  cy.wrap(engine, { log: false }).should((e) => {
    expect(e.readLines()[0]?.text).to.equal(text);
  });
}

describe("Wide characters in the web terminal", () => {
  let opened: TerminalEngine | null = null;
  afterEach(() => {
    opened?.dispose();
    opened = null;
  });

  it("draws an emoji two cells wide", () => {
    // Given
    anOpenXtermEngine().then((engine) => {
      opened = engine;

      // When — an emoji, then a marker placed at the third column
      engine.write(`😀${MOVE_TO_COLUMN_3}Z`);

      // Then — the marker sits right after the emoji
      expectFirstLine(engine, "😀Z");
    });
  });

  it("draws a CJK character two cells wide", () => {
    // Given
    anOpenXtermEngine().then((engine) => {
      opened = engine;

      // When
      engine.write(`界${MOVE_TO_COLUMN_3}Z`);

      // Then
      expectFirstLine(engine, "界Z");
    });
  });

  it("keeps a narrow character one cell wide", () => {
    // Given
    anOpenXtermEngine().then((engine) => {
      opened = engine;

      // When — a plain letter leaves column 2 blank before the marker
      engine.write(`a${MOVE_TO_COLUMN_3}Z`);

      // Then
      expectFirstLine(engine, "a Z");
    });
  });
});
