/**
 * E2E: Selection persistence in Select mode over LiveKit RPC.
 *
 * Bug: pressing Down arrow in a Select question briefly highlights the next
 * option but the selection resets to the first option after periodic re-renders.
 *
 * This test connects to tddy-demo, waits for the scope Select question,
 * sends Down arrow, waits for render cycles, and asserts the selection held.
 *
 * Requires: LIVEKIT_TESTKIT_WS_URL, tddy-demo built.
 * Skipped when LIVEKIT_TESTKIT_WS_URL is not set.
 */
import type { TerminalSessionResult } from "../support/commands";

const STORY_ID = "components-ghosttyterminal--live-kit-connected";

describe("Ghostty Selection Persistence", () => {
  let session: TerminalSessionResult = {} as TerminalSessionResult;

  before(function () {
    // Use a prompt WITHOUT SKIP_QUESTIONS so the workflow pauses at the scope
    // Select question — this gives us the selection UI to test.
    cy.startTerminalSession({ kind: "terminal", prompt: "Build auth" }).then((result) => {
      session = result;
    });
  });

  after(() => {
    if (session.serverLogPath) cy.dumpServerLog(session.serverLogPath);
    cy.task("stopTerminalServer");
  });

  it("Down arrow selection persists through periodic re-renders", () => {
    // Given — connect and wait for the Select question to appear
    cy.visitGhosttyStory({ storyId: STORY_ID, url: session.url, token: session.clientToken, roomName: session.roomName });
    cy.connectAndWaitForTerminal();

    cy.waitForBufferText("Email/password");

    // Then — initial highlight is on first option
    cy.get("[data-testid='terminal-highlighted-line']", { timeout: 5000 }).should(($el) => {
      expect($el.text()).to.include("Email/password");
    });

    // When — send Down arrow to move to next option
    cy.get("[data-testid='ghostty-terminal']").click();
    cy.get("[data-testid='ghostty-terminal']").type("{downarrow}");

    // Wait for periodic render ticks (200ms each) to verify the selection held.
    // This is the nature of the regression being tested — selection resets after renders.
    // eslint-disable-next-line cypress/no-unnecessary-waiting
    cy.wait(2000); // justified: regression only manifests after 1-2 render cycles (~200ms each)

    cy.get("[data-testid='ghostty-terminal']").screenshot("oauth-selected");

    // Then — highlight moved to "OAuth" and stayed there
    cy.get("[data-testid='terminal-highlighted-line']").should(($el) => {
      const line = $el.text();
      expect(
        line,
        `After one Down arrow, highlighted line must be "OAuth" (index 1). Got: "${line}"`,
      ).to.include("OAuth");
    });
  });
});
