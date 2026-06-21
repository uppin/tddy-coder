/**
 * E2E: Terminal should not show duplicate status bar panes when multiple frames stream.
 *
 * Bug: when the backend streams multiple frames the scrollback accumulates old frames,
 * causing duplicate TUI panes to appear above the current viewport.
 *
 * Requires: LIVEKIT_TESTKIT_WS_URL, tddy-demo built.
 * Skipped when LIVEKIT_TESTKIT_WS_URL is not set.
 */
import type { TerminalSessionResult } from "../support/commands";

const STORY_ID = "components-ghosttyterminal--live-kit-connected";

describe("Ghostty Fullscreen No Duplicate", () => {
  let session: TerminalSessionResult = {} as TerminalSessionResult;

  before(function () {
    cy.startTerminalSession({ kind: "terminal", prompt: "Build auth" }).then((result) => {
      session = result;
    });
  });

  after(() => {
    cy.task("stopTerminalServer");
  });

  it("shows the status bar exactly once when multiple frames have been streamed", () => {
    // Given
    cy.visitGhosttyStory({ storyId: STORY_ID, url: session.url, token: session.clientToken, roomName: session.roomName });
    cy.connectAndWaitForTerminal();
    cy.waitForBufferText("Email/password");

    // Wait for a second round of periodic re-renders to accumulate scrollback
    // eslint-disable-next-line cypress/no-unnecessary-waiting
    cy.wait(2000); // justified: regression only visible after multiple streaming frames

    // Then — `Goal:` (TUI status line marker) appears exactly once — no duplicate panes
    cy.get("[data-testid='terminal-buffer-text']").should(($el) => {
      const matches = $el.text().match(/Goal:/g);
      expect(
        matches?.length ?? 0,
        "status bar should appear only once (no duplicate panes from scrollback)",
      ).to.equal(1);
    });
  });
});
