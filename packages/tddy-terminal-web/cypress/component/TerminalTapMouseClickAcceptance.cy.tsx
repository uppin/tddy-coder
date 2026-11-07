import { aGhosttyTerminal } from "../support/drivers/ghosttyTerminalDriver";

/**
 * A tap on a terminal pane must reach the terminal as a mouse click on mobile.
 *
 * Today it does not. `preventFocusOnTap` (mobile with the keyboard closed) calls
 * `preventDefault()` on `touchstart`, which suppresses the browser's compatibility mouse
 * events — so a tap produces no `mousedown`, `mouseup` or `click` at all. The only touch
 * path that emits anything is the SGR forwarding in `GhosttyTerminal`, and that is gated on
 * `hasMouseTracking()`. Agents that never enable mouse reporting (the Claude CLI emits no
 * DECSET 1000/1002/1003/1006) therefore see nothing whatsoever from a tap, and neither does
 * ghostty-web's own click handling.
 *
 * These specs pin the tap-as-click contract: the gesture arrives as ordinary mouse events,
 * at the coordinates the finger touched, with no mouse tracking enabled by the TUI.
 */
describe("Terminal tap as mouse click (mobile)", () => {
  it("delivers a mousedown followed by a mouseup when the user taps the terminal", () => {
    // Given — a mobile terminal (focus prevention on) whose TUI enabled no mouse tracking
    const driver = aGhosttyTerminal({ preventFocusOnTap: true }).mount();
    driver.expectExists().expectCanvasExists();
    driver.recordMouseEvents();

    // When — the user taps the pane
    driver.simulateTouchTap();

    // Then
    driver.expectMouseDownThenMouseUp();
  });

  it("delivers a click event when the user taps the terminal", () => {
    // Given — a mobile terminal (focus prevention on) whose TUI enabled no mouse tracking
    const driver = aGhosttyTerminal({ preventFocusOnTap: true }).mount();
    driver.expectExists().expectCanvasExists();
    driver.recordMouseEvents();

    // When — the user taps the pane
    driver.simulateTouchTap();

    // Then
    driver.expectClickDispatchedOnce();
  });

  it("delivers the click at the coordinates the finger touched, not the pane centre", () => {
    // Given — a mobile terminal (focus prevention on) whose TUI enabled no mouse tracking
    const driver = aGhosttyTerminal({ preventFocusOnTap: true }).mount();
    driver.expectExists().expectCanvasExists();
    driver.recordMouseEvents();

    // When — the user taps left of centre, low in the pane
    driver.simulateTouchTapAt(0.25, 0.75);

    // Then
    driver.expectMouseDownAtTapPoint();
  });

  it("delivers no click when the user drags one finger to scroll", () => {
    // Given — a mobile terminal (focus prevention on) whose TUI enabled no mouse tracking
    const driver = aGhosttyTerminal({ preventFocusOnTap: true }).mount();
    driver.expectExists().expectCanvasExists();
    driver.recordMouseEvents();

    // When — the user drags a finger down the pane to pull earlier output into view
    driver.dragDownOneFinger();

    // Then
    driver.expectNoClickDispatched();
  });

  it("delivers no click when the user touches the terminal with two fingers", () => {
    // Given — a mobile terminal (focus prevention on) whose TUI enabled no mouse tracking
    const driver = aGhosttyTerminal({ preventFocusOnTap: true }).mount();
    driver.expectExists().expectCanvasExists();
    driver.recordMouseEvents();

    // When — a second finger joins before both lift (pinch / two-finger tap)
    driver.tapWithTwoFingers();

    // Then
    driver.expectNoClickDispatched();
  });

  it("reports a tap once to a TUI that enabled mouse tracking", () => {
    // Given — a mobile terminal whose TUI enabled SGR mouse tracking (DECSET 1000 + 1006)
    const driver = aGhosttyTerminal({
      preventFocusOnTap: true,
      initialContent: "\x1b[?1000h\x1b[?1006h",
      withHandleCapture: true,
    }).mount();
    driver.expectExists().expectCanvasExists().expectReportingMouseToTui();

    // When — the user taps the pane
    driver.simulateTouchTap();

    // Then — the touch forwarding reports it, and the synthesised click must not report it again
    driver.expectSgrPressAndReleaseReportedOnce();
  });
});
