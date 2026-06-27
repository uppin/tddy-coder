/**
 * Bug reproduction: terminal keyboard shortcuts on the desktop browser.
 *
 * Reported behaviour (two related defects):
 *   1. The sticky shortcut overlay (ShortcutDrawer) listing key presses such as
 *      Shift+Tab is only rendered when the mobile keyboard is shown, so it is
 *      invisible on a desktop browser even when the session has shortcuts.
 *   2. Pressing those combinations on a real keyboard while the terminal window
 *      is focused (e.g. Shift+Tab, Alt+M) does nothing — the key sequence is
 *      never forwarded to the remote PTY on desktop.
 *
 * Expected behaviour:
 *   - The overlay is visible on the desktop browser whenever the session
 *     exposes shortcuts.
 *   - A physical Shift+Tab while the terminal is focused sends the reverse-tab
 *     sequence (ESC [ Z) to the PTY; a physical Alt+M sends the meta sequence
 *     (ESC m).
 *
 * These tests fail today because no desktop overlay is rendered and no
 * desktop keydown handler forwards the combinations to the PTY input channel.
 */

import { aGhosttyTerminal } from "../support/drivers/ghosttyTerminalDriver";
import { aGhosttyTerminalLiveKit } from "../support/drivers/ghosttyTerminalLiveKitDriver";

// GhosttyTerminal's `onData` is the PTY input channel: GhosttyTerminalLiveKit
// wires `onData` straight into `enqueueTerminalInput`, which is drained to the
// remote terminal. Asserting on `onData` therefore asserts "sent to the PTY".
const REVERSE_TAB_SEQUENCE = "\x1b[Z"; // Shift+Tab → CSI Z
const ALT_M_META_SEQUENCE = "\x1bm"; // Alt+M → ESC + "m" (xterm meta encoding)

const CLAUDE_CLI_SHORTCUTS = [
  { label: "Shift+Tab", keys: ["Shift", "Tab"] },
  { label: "Alt+M", keys: ["Alt", "M"] },
  { label: "Escape", keys: ["Escape"] },
];

describe("Desktop browser terminal shortcuts", () => {
  beforeEach(() => {
    cy.viewport(1280, 800);
  });

  it("shows the sticky shortcut overlay on the desktop browser when the session has shortcuts", () => {
    // Given — a desktop session (mobile keyboard not shown) that exposes shortcuts
    aGhosttyTerminalLiveKit({
      showMobileKeyboard: false,
      mobileShortcuts: CLAUDE_CLI_SHORTCUTS,
    })
      .mount()
      // Then — the overlay listing the shortcuts is visible
      .expectShortcutDrawerExists();
  });

  it("sends the reverse-tab sequence to the PTY when Shift+Tab is pressed on the focused terminal", () => {
    // Given — a focused desktop terminal
    const driver = aGhosttyTerminal({ onData: cy.stub().as("onData") }).mount();
    driver.expectExists().click();

    // When — the user presses Shift+Tab on the physical keyboard
    driver.pressPhysicalKey({ key: "Tab", code: "Tab", shiftKey: true });

    // Then — the reverse-tab escape sequence reaches the PTY input channel
    driver.expectOnDataCalledWith(REVERSE_TAB_SEQUENCE);
  });

  it("sends the meta sequence to the PTY when Alt+M is pressed on the focused terminal", () => {
    // Given — a focused desktop terminal
    const driver = aGhosttyTerminal({ onData: cy.stub().as("onData") }).mount();
    driver.expectExists().click();

    // When — the user presses Alt+M on the physical keyboard
    driver.pressPhysicalKey({ key: "m", code: "KeyM", altKey: true });

    // Then — the Alt+M meta sequence reaches the PTY input channel
    driver.expectOnDataCalledWith(ALT_M_META_SEQUENCE);
  });
});
