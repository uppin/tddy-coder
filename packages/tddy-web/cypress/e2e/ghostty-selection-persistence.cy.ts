/**
 * E2E: Selection persistence in Select mode over LiveKit RPC.
 *
 * Bug: pressing Down arrow in a Select question briefly highlights the next
 * option but the selection resets to the first option after periodic re-renders.
 * The user sees the highlight "blink" then snap back.
 *
 * This test:
 * 1. Connects to tddy-demo via LiveKit (same as ghostty-terminal.cy.ts)
 * 2. Waits for the scope Select question to appear (with "Email/password")
 * 3. Sends Down arrow to move selection to "OAuth"
 * 4. Waits 2 seconds for periodic renders to cycle
 * 5. Asserts the highlighted line (reverse-video) is "OAuth", not "Email/password"
 *
 * Uses the ghostty-web cell-level API (IBufferCell.isInverse()) to detect
 * which line has the selection highlight, exposed via [data-testid='terminal-highlighted-line'].
 *
 * Requires: LIVEKIT_TESTKIT_WS_URL, tddy-demo built
 */
describe("Ghostty Selection Persistence", () => {
  let serverUrl: string;
  let clientToken: string;
  let roomName: string;
  let serverLogPath: string | undefined;

  before(function () {
    if (!Cypress.env("LIVEKIT_TESTKIT_WS_URL")) {
      this.skip();
      return;
    }
    // Use a prompt WITHOUT SKIP_QUESTIONS so the workflow pauses at the
    // scope Select question — this gives us the selection UI to test.
    return cy.task("startTerminalServer", { prompt: "Build auth" }).then((result) => {
      const r = result as {
        url: string;
        clientToken: string;
        roomName: string;
        serverLogPath?: string;
      };
      serverUrl = r.url;
      clientToken = r.clientToken;
      roomName = r.roomName;
      serverLogPath = r.serverLogPath;
    });
  });

  after(() => {
    if (serverLogPath) {
      cy.task("readLogFile", serverLogPath).then((content) => {
        cy.task(
          "log",
          `\n--- tddy-demo server log ---\n${content}\n--- End ---\n`
        );
      });
    }
    cy.task("stopTerminalServer");
  });

  it("Down arrow selection persists after periodic re-renders", () => {
    // Load the LiveKitConnected story with showBufferTextForTest=true
    const storyUrl = `/iframe.html?id=components-ghosttyterminal--live-kit-connected&url=${encodeURIComponent(serverUrl)}&token=${encodeURIComponent(clientToken)}&roomName=${encodeURIComponent(roomName)}`;
    cy.visit(storyUrl);

    // Wait for LiveKit connection
    cy.get("[data-testid='connection-status-dot']", { timeout: 15000 })
      .should("be.visible")
      .and("have.attr", "data-connection-status", "connected");
    cy.get("[data-testid='livekit-status']").should("not.be.visible");

    cy.get("[data-testid='ghostty-terminal']", { timeout: 5000 }).should(
      "exist"
    );

    // Wait for first output (terminal rendering started)
    cy.get("[data-testid='first-output-received']", { timeout: 15000 }).should(
      "exist"
    );

    // Wait for the Select question to appear in the buffer.
    // The workflow starts with "Build auth", reaches the Scope question.
    cy.get("[data-testid='terminal-buffer-text']", { timeout: 20000 }).should(
      ($el) => {
        const text = $el.text();
        expect(text).to.include("Email/password");
      }
    );

    // Verify initial selection is on the first option (Email/password has inverse).
    cy.get("[data-testid='terminal-highlighted-line']", {
      timeout: 5000,
    }).should(($el) => {
      const line = $el.text();
      expect(line).to.include("Email/password");
    });

    // Send Down arrow to move selection from "Email/password" to "OAuth".
    cy.get("[data-testid='ghostty-terminal']").click();
    cy.get("[data-testid='ghostty-terminal']").type("{downarrow}");

    // Wait for several periodic render ticks (200ms each) to verify persistence.
    // The bug causes selection to reset after 1-2 render cycles.
    cy.wait(2000);

    // Capture screenshot of the terminal with OAuth selected.
    cy.get("[data-testid='ghostty-terminal']").screenshot("oauth-selected");

    // Assert the highlighted line (reverse-video via isInverse()) is exactly "OAuth".
    // One Down from "Email/password" (index 0) must land on "OAuth" (index 1).
    // If this lands on "Other" (index 2), that means duplicate key events were sent
    // — a real bug (e.g. double onData registration in GhosttyTerminal).
    cy.get("[data-testid='terminal-highlighted-line']").should(($el) => {
      const line = $el.text();
      expect(line).to.include(
        "OAuth",
        `After one Down arrow, the highlighted line must be "OAuth" (index 1). ` +
          `Got: "${line}"`
      );
    });
  });
});
