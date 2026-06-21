/**
 * E2E acceptance: Ghostty terminal content from tddy-demo over LiveKit.
 *
 * Requires: LIVEKIT_TESTKIT_WS_URL, tddy-demo built (cargo build -p tddy-demo).
 *
 * Skipped when LIVEKIT_TESTKIT_WS_URL is not set.
 */
import type { TerminalSessionResult } from "../support/commands";

const STORY_ID = "components-ghosttyterminal--live-kit-connected";

describe("Ghostty Terminal E2E", () => {
  let session: TerminalSessionResult = {} as TerminalSessionResult;

  before(function () {
    cy.startTerminalSession({ kind: "terminal" }).then((result) => {
      session = result;
    });
  });

  after(function () {
    if (this.currentTest?.state === "failed" && session.serverLogPath) {
      cy.dumpServerLog(session.serverLogPath);
    }
    cy.task("stopTerminalServer");
  });

  it("displays tddy-demo terminal output in Ghostty through LiveKit", () => {
    // Given
    cy.visitGhosttyStory({ storyId: STORY_ID, url: session.url, token: session.clientToken, roomName: session.roomName });

    // When — wait for full LiveKit connection
    cy.connectAndWaitForTerminal();

    // Then — RPC has streamed bytes from tddy-demo
    cy.get("[data-testid='streamed-byte-count']", { timeout: 30000 }).should(($el) => {
      expect(parseInt($el.text(), 10)).to.be.greaterThan(0, "RPC should have streamed bytes from tddy-demo");
    });

    // Then — terminal buffer has content
    cy.get("[data-testid='terminal-buffer-text']", { timeout: 20000 }).should(($el) => {
      expect($el.text().length).to.be.greaterThan(0, "terminal buffer should have content from tddy-demo");
    });
  });

  it("shows the coder-unavailable banner and disables input when the server participant leaves", () => {
    // Given
    cy.visitGhosttyStory({ storyId: STORY_ID, url: session.url, token: session.clientToken, roomName: session.roomName });
    cy.connectAndWaitForTerminal();

    // When — kill the server
    cy.task("stopTerminalServer");

    // Then — coder-unavailable banner appears
    cy.get("[data-testid='terminal-coder-unavailable']", { timeout: 45000 })
      .should("be.visible")
      .and(($el) => {
        expect($el.text().trim().length).to.be.greaterThan(0, "banner should explain the session ended");
      });

    // Then — terminal marks session inactive
    cy.get("[data-testid='ghostty-terminal']").should("have.attr", "data-session-active", "false");
  });
});
