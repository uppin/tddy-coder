/**
 * E2E: A second browser window connecting to the same LiveKit room with mobile
 * screen dimensions must not blank the first (desktop) window's terminal.
 *
 * Bug: reconnecting to a session in tddy-web results in a blank terminal
 * (only cursor blinking). The second RPC TUI session somehow affects the first.
 *
 * This test uses window.open() to spawn a second browser window that connects
 * to the same tddy-demo server via the same LiveKit room with the same client
 * identity — simulating the real reconnection scenario.
 *
 * Requires: LIVEKIT_TESTKIT_WS_URL, tddy-demo built
 */
describe("Ghostty Concurrent Sessions — Two Browser Windows", () => {
  let serverUrl: string;
  let clientToken: string;
  let roomName: string;

  before(function () {
    if (!Cypress.env("LIVEKIT_TESTKIT_WS_URL")) {
      this.skip();
      return;
    }
    return cy.task("startTerminalServer", { prompt: "Build auth" }).then((result) => {
      const r = result as {
        url: string;
        clientToken: string;
        roomName: string;
      };
      serverUrl = r.url;
      clientToken = r.clientToken;
      roomName = r.roomName;
    });
  });

  after(() => {
    cy.task("stopTerminalServer");
  });

  it("mobile window connecting with same identity does not blank desktop terminal", () => {
    const storyUrl = `/iframe.html?id=components-ghosttyterminal--live-kit-connected&url=${encodeURIComponent(serverUrl)}&token=${encodeURIComponent(clientToken)}&roomName=${encodeURIComponent(roomName)}`;

    // Desktop window — full viewport
    cy.visit(storyUrl);

    cy.get("[data-testid='connection-status-dot']", { timeout: 25000 })
      .should("be.visible")
      .and("have.attr", "data-connection-status", "connected");

    cy.get("[data-testid='first-output-received']", { timeout: 15000 }).should("exist");

    cy.get("[data-testid='terminal-buffer-text']", { timeout: 20000 }).should(($el) => {
      const text = $el.text();
      expect(text).to.include("Email/password");
    });

    // Open a second browser window with the SAME token (same identity)
    // and mobile viewport dimensions, simulating a phone reconnection.
    cy.window().then((win) => {
      const mobileWidth = 375;
      const mobileHeight = 667;
      win.open(
        storyUrl,
        "mobile-terminal",
        `width=${mobileWidth},height=${mobileHeight},menubar=no,toolbar=no`
      );
    });

    // Wait for the second window's LiveKit connection to establish
    // and any cross-session effects to propagate.
    cy.wait(5000);

    // The desktop terminal must still show the user question — not go blank.
    cy.get("[data-testid='terminal-buffer-text']").should(($el) => {
      const text = $el.text();
      expect(text).to.include(
        "Email/password",
        "Desktop terminal must not go blank after mobile window connects with same identity"
      );
    });

    // Status bar must still appear (not blanked)
    cy.get("[data-testid='terminal-buffer-text']").should(($el) => {
      const text = $el.text();
      const goalMatches = text.match(/Goal:/g);
      expect(
        goalMatches?.length ?? 0,
        "Desktop terminal should still have a status bar"
      ).to.be.greaterThan(0);
    });

    // Connection must still be active (not evicted)
    cy.get("[data-testid='connection-status-dot']")
      .should("be.visible")
      .and("have.attr", "data-connection-status", "connected");
  });
});
