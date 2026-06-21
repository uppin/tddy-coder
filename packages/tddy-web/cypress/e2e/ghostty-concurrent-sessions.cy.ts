/**
 * E2E: A second browser window connecting to the same LiveKit room with mobile
 * dimensions must not blank the first (desktop) window's terminal.
 *
 * Bug: reconnecting to a session in tddy-web resulted in a blank terminal.
 *
 * Requires: LIVEKIT_TESTKIT_WS_URL, tddy-demo built.
 * Skipped when LIVEKIT_TESTKIT_WS_URL is not set.
 */
import type { TerminalSessionResult } from "../support/commands";

const STORY_ID = "components-ghosttyterminal--live-kit-connected";

describe("Ghostty Concurrent Sessions — Two Browser Windows", () => {
  let session: TerminalSessionResult = {} as TerminalSessionResult;

  before(function () {
    cy.startTerminalSession({ kind: "terminal", prompt: "Build auth" }).then((result) => {
      session = result;
    });
  });

  after(() => {
    cy.task("stopTerminalServer");
  });

  it("mobile window connecting with the same identity does not blank the desktop terminal", () => {
    // Given — desktop window connected and showing content
    const storyUrl = `/iframe.html?id=${STORY_ID}&url=${encodeURIComponent(session.url)}&token=${encodeURIComponent(session.clientToken)}&roomName=${encodeURIComponent(session.roomName)}`;
    cy.visit(storyUrl);
    cy.connectAndWaitForTerminal();
    cy.waitForBufferText("Email/password");

    // When — open a second window with mobile viewport + same token (same identity)
    cy.window().then((win) => {
      win.open(
        storyUrl,
        "mobile-terminal",
        "width=375,height=667,menubar=no,toolbar=no",
      );
    });

    // Wait for the second window's connection to establish and any cross-session
    // effects to propagate before asserting the desktop state
    // eslint-disable-next-line cypress/no-unnecessary-waiting
    cy.wait(5000); // justified: second window needs to complete its LiveKit handshake

    // Then — desktop terminal still shows user question (not blank)
    cy.get("[data-testid='terminal-buffer-text']").should(($el) => {
      expect(
        $el.text(),
        "Desktop terminal must not go blank after mobile window connects with same identity",
      ).to.include("Email/password");
    });

    // Then — status bar still present (not blanked)
    cy.get("[data-testid='terminal-buffer-text']").should(($el) => {
      expect(
        ($el.text().match(/Goal:/g)?.length ?? 0),
        "Desktop terminal should still have a status bar",
      ).to.be.greaterThan(0);
    });

    // Then — connection still active (not evicted)
    cy.get("[data-testid='connection-status-dot']")
      .should("be.visible")
      .and("have.attr", "data-connection-status", "connected");
  });
});
