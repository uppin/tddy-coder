/**
 * Each terminal owns its emulator: a pane opened after another one closed starts on a blank screen.
 *
 * Regression: on ghostty-web's process-wide WASM instance, a terminal created after another was
 * freed was handed the freed one's memory unzeroed — a new session's pane opened on the deleted
 * session's screen, its own output drawn over it.
 */

import React, { useImperativeHandle, useRef, useState, forwardRef } from "react";
import { mount } from "cypress/react";
import { GhosttyTerminal, type GhosttyTerminalHandle } from "../../src/components/GhosttyTerminal";

const OLD_SESSION_OUTPUT = Array.from(
  { length: 60 },
  (_, i) => `OLD SESSION line ${i} wixel-pdf/font_repair.rs`,
).join("\r\n");

type SwitcherHandle = {
  showNewSession(): void;
  bufferText(): string | undefined;
};

/** Renders one session's pane at a time, keyed by session — as `SessionMainPane` does. */
const SessionPaneSwitcher = forwardRef<SwitcherHandle>(function SessionPaneSwitcher(_, ref) {
  const [session, setSession] = useState<"old" | "new">("old");
  const terminal = useRef<GhosttyTerminalHandle>(null);
  useImperativeHandle(ref, () => ({
    showNewSession: () => setSession("new"),
    bufferText: () => terminal.current?.getBufferText?.(),
  }));
  return (
    <div style={{ width: 640, height: 320 }}>
      <GhosttyTerminal
        key={session}
        ref={terminal}
        initialContent={session === "old" ? OLD_SESSION_OUTPUT : "NEW SESSION banner"}
      />
    </div>
  );
});

it("opens a new session's terminal on a blank screen after the previous session's terminal closed", () => {
  // Given — a session's terminal that has drawn a screenful of output
  const switcher = React.createRef<SwitcherHandle>();
  mount(<SessionPaneSwitcher ref={switcher} />);
  cy.wrap(switcher).should((s) => {
    expect(s.current?.bufferText()).to.contain("OLD SESSION line 59");
  });

  // When — that pane closes and the next session's pane opens
  cy.then(() => switcher.current?.showNewSession());

  // Then — the new terminal shows only its own output
  cy.wrap(switcher).should((s) => {
    const text = s.current?.bufferText() ?? "";
    expect(text).to.contain("NEW SESSION banner");
    expect(text).not.to.contain("OLD SESSION");
  });
});
