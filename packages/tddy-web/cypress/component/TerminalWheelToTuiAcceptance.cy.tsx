import { aWheelTuiSession, type WheelTuiSessionOptions } from "../support/drivers/wheelTuiSessionDriver";

/**
 * In a full-screen TUI that tracks the mouse, the wheel belongs to the application: it must arrive
 * as an SGR wheel report (button 64 up / 65 down) and the application must be able to act on it.
 *
 * Reported symptom: scrolling over a TUI (the Claude CLI) shows an attempted out-of-TUI scroll that
 * is immediately reset, and the TUI never indicates that it received the wheel. These specs run a
 * real session pane against a TUI that counts what reached it, across the combinations a real TUI
 * uses: alternate or normal screen, the tracking mode it requests, whether it launched before the
 * terminal attached, and whether the connection serves scrollback at all.
 */

const aFullScreenTui = (overrides: Partial<WheelTuiSessionOptions> = {}): WheelTuiSessionOptions => ({
  alternateScreen: true,
  mouse: "1002",
  canServeHistory: true,
  launchedBeforeAttach: false,
  ...overrides,
});

describe("Mouse wheel over a TUI that tracks the mouse", () => {
  it("reaches a full-screen TUI as an SGR wheel report", () => {
    // Given — a TUI on the alternate screen that requested button-event tracking (1002 + 1006)
    const session = aWheelTuiSession(aFullScreenTui()).mount().launchTui();

    // When — the user scrolls up one notch over it
    session.wheelUp();

    // Then — the application counted one wheel-up report and no arrow keys
    session.expectTuiReceivedWheel({ up: 1, down: 0 });
  });

  it("reaches a TUI that is still drawing a second wheel notch down after a repaint", () => {
    // Given — the TUI has already repainted once in answer to a wheel-up
    const session = aWheelTuiSession(aFullScreenTui()).mount().launchTui();
    session.wheelUp().expectTuiReceivedWheel({ up: 1, down: 0 });

    // When — the user scrolls back down
    session.wheelDown();

    // Then — the application counted both notches, each as a wheel report
    session.expectTuiReceivedWheel({ up: 1, down: 1 });
  });

  it("reaches a TUI on the normal screen that tracks the mouse", () => {
    // Given — a TUI drawing inline (no alternate screen) with tracking on
    const session = aWheelTuiSession(aFullScreenTui({ alternateScreen: false })).mount().launchTui();

    // When
    session.wheelUp();

    // Then
    session.expectTuiReceivedWheel({ up: 1, down: 0 });
  });

  it("reaches a TUI that requested click-only tracking (1000)", () => {
    // Given
    const session = aWheelTuiSession(aFullScreenTui({ mouse: "1000" })).mount().launchTui();

    // When
    session.wheelUp();

    // Then
    session.expectTuiReceivedWheel({ up: 1, down: 0 });
  });

  it("reaches a TUI that requested any-event tracking (1003)", () => {
    // Given
    const session = aWheelTuiSession(aFullScreenTui({ mouse: "1003" })).mount().launchTui();

    // When
    session.wheelUp();

    // Then
    session.expectTuiReceivedWheel({ up: 1, down: 0 });
  });

  it("reaches a TUI that was already running when the terminal attached", () => {
    // Given — the TUI's mode setup arrives inside the attach replay, not as live output
    const session = aWheelTuiSession(aFullScreenTui({ launchedBeforeAttach: true })).mount().launchTui();

    // When
    session.wheelUp();

    // Then
    session.expectTuiReceivedWheel({ up: 1, down: 0 });
  });

  it("reaches a TUI over a connection that serves no scrollback", () => {
    // Given — a feed without history (a local attach), so the pane installs no history gate
    const session = aWheelTuiSession(aFullScreenTui({ canServeHistory: false })).mount().launchTui();

    // When
    session.wheelUp();

    // Then — still an SGR report, not the arrow key ghostty-web emulates in the alternate screen
    session.expectTuiReceivedWheel({ up: 1, down: 0 });
  });
});
