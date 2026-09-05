import { aGhosttyTerminal } from "../support/drivers/ghosttyTerminalDriver";

/**
 * A one-finger drag on a mobile terminal must reach whoever owns the scrolling, exactly as the
 * wheel does on desktop.
 *
 * Desktop already gates the wheel three ways (`GhosttyTerminalSession` + ghostty-web's own handler):
 * a mouse-tracking TUI gets an SGR wheel report, a pager in the alternate screen gets ghostty-web's
 * Up/Down emulation, and only the normal screen scrolls the emulator's scrollback (the live pane of
 * the history double buffer, at `scrollback: 0`, turning that into the lazy-history fill).
 *
 * Touch had no such gate: every drag drove `scrollLines` on the pane. In a full-screen TUI — the
 * Claude CLI, which runs in the alternate screen with mouse tracking on — that scrolls a pane whose
 * scrollback is empty by design, so the gesture does nothing and the double buffer is the only way
 * back through the output, while the TUI's own scrollback stays out of reach. These specs pin the
 * routing: in the alternate screen the terminal (and the application in it) owns the scroll.
 */

/** Deterministic overflow content: row-001 .. row-<count>, each on its own line (CRLF). */
function numberedLines(count: number): string {
  return Array.from(
    { length: count },
    (_, i) => `row-${String(i + 1).padStart(3, "0")}`,
  ).join("\r\n");
}

describe("Terminal touch scroll routing (mobile)", () => {
  it("reports a one-finger drag to a full-screen TUI that tracks the mouse as an SGR wheel event", () => {
    // Given — the alternate screen (DEC 1049) with mouse tracking on (DEC 1002 + 1006): the Claude
    // CLI's TUI, which scrolls its own transcript in response to the wheel.
    const driver = aGhosttyTerminal({
      preventFocusOnTap: true,
      initialContent: `\x1b[?1049h${numberedLines(60)}\x1b[?1002h\x1b[?1006h`,
      withHandleCapture: true,
    }).mount();
    driver
      .expectExists()
      .expectCanvasExists()
      .expectInAlternateScreen()
      .expectReportingMouseToTui();

    // When — the user drags one finger down to pull earlier output into view
    driver.dragDownOneFinger();

    // Then — the TUI received wheel-up mouse reports (SGR button 64), not an Up-arrow key: the
    // gesture belongs to the application, the same way the desktop wheel gate routes it.
    driver.expectSentToApp("\x1b[<64;").expectDidNotSendToApp("\x1b[A");
  });

  it("hands a one-finger drag in the alternate screen to the terminal when the TUI tracks no mouse", () => {
    // Given — the alternate screen without mouse tracking: a pager such as `less`, which scrolls on
    // the arrow keys ghostty-web emulates for the wheel.
    const driver = aGhosttyTerminal({
      preventFocusOnTap: true,
      initialContent: `\x1b[?1049h${numberedLines(60)}`,
      withHandleCapture: true,
    }).mount();
    driver.expectExists().expectCanvasExists().expectInAlternateScreen();

    // When — the user drags one finger down to pull earlier output into view
    driver.dragDownOneFinger();

    // Then — the drag reached the terminal's own wheel handling, which emulates Up arrows for the
    // pager, instead of being swallowed by a scrollback the alternate screen does not have.
    driver.expectSentToApp("\x1b[A");
  });

  it("scrolls the terminal's own scrollback on a one-finger drag in the normal screen", () => {
    // Given — the normal screen with more output than fits: the emulator's scrollback is the thing
    // being scrolled, and nothing about the gesture belongs to the application.
    const driver = aGhosttyTerminal({
      preventFocusOnTap: true,
      initialContent: numberedLines(200),
      withHandleCapture: true,
    }).mount();
    driver.expectExists().expectCanvasExists();

    // When — the user drags one finger down to pull earlier output into view
    driver.dragDownOneFinger();

    // Then — the viewport moves back through the scrollback, and no key or mouse report is sent.
    driver
      .expectRevealsEarlierOutput()
      .expectDidNotSendToApp("\x1b[A")
      .expectDidNotSendToApp("\x1b[<64;");
  });
});
