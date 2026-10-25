/**
 * `getBufferText` is polled several times a second by every mounted session pane, so a terminal
 * whose content has not changed must answer from what it last read — and one whose content has
 * changed must never answer stale.
 *
 * Regression: each poll serialized the whole buffer cell by cell, scrollback included. A history
 * page terminal holds up to 50 000 lines, and re-reading it five times a second kept Tddy Desktop's
 * WebView at a full core with no output arriving.
 */

import React from "react";
import { mount } from "cypress/react";
import {
  GhosttyTerminal,
  type GhosttyTerminalHandle,
} from "../../src/components/GhosttyTerminal";

const HISTORY_LINES = 20000;

const mountTerminal = (props: { scrollback?: number } = {}) => {
  const terminal = React.createRef<GhosttyTerminalHandle>();
  const onReady = cy.stub().as("ready");
  mount(
    <div style={{ width: 640, height: 320 }}>
      <GhosttyTerminal
        ref={terminal}
        scrollback={props.scrollback}
        onReady={onReady}
      />
    </div>,
  );
  cy.get("@ready").should("have.been.called");
  return terminal;
};

const bufferTextOf = (terminal: React.RefObject<GhosttyTerminalHandle>) =>
  terminal.current?.getBufferText?.() ?? "";

it("shows output written after a previous read", () => {
  // Given — a terminal whose buffer text has been read once
  const terminal = mountTerminal();
  cy.then(() => terminal.current?.write("first line\r\n"));
  cy.wrap(terminal).should((t) =>
    expect(bufferTextOf(t)).to.contain("first line"),
  );

  // When — more output arrives
  cy.then(() => terminal.current?.write("second line\r\n"));

  // Then — the next read carries it
  cy.wrap(terminal).should((t) => {
    const text = bufferTextOf(t);
    expect(text).to.contain("first line");
    expect(text).to.contain("second line");
  });
});

it("answers repeated reads of an unchanged terminal without re-reading its scrollback", () => {
  // Given — a history-sized terminal, read once
  const terminal = mountTerminal({ scrollback: 50000 });
  cy.then(() => {
    const lines = Array.from(
      { length: HISTORY_LINES },
      (_, i) => `history line ${i}`,
    );
    terminal.current?.write(lines.join("\r\n"));
  });
  cy.wrap(terminal).should((t) =>
    expect(bufferTextOf(t)).to.contain(`history line ${HISTORY_LINES - 1}`),
  );

  cy.then(() => {
    // When — it is polled a hundred more times with nothing written in between
    const fullRead = (() => {
      terminal.current?.write("x");
      const start = performance.now();
      bufferTextOf(terminal);
      return performance.now() - start;
    })();
    const start = performance.now();
    for (let i = 0; i < 100; i++) bufferTextOf(terminal);
    const hundredPolls = performance.now() - start;

    // Then — all hundred together cost less than the one read that followed a write
    expect(
      hundredPolls,
      `100 polls took ${hundredPolls}ms, one full read ${fullRead}ms`,
    ).to.be.lessThan(fullRead);
  });
});
