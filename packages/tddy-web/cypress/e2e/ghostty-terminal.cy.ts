/**
 * Strip ANSI escape sequences (CSI, OSC) to get readable text for assertions.
 */
function stripAnsi(text: string): string {
  return text
    .replace(/\x1b\[[?0-9;]*[a-zA-Z]/g, "")
    .replace(/\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g, "");
}

/**
 * E2E acceptance test: Ghostty terminal content from tddy-demo over LiveKit.
 *
 * Requires:
 * - LIVEKIT_TESTKIT_WS_URL (from ./run-livekit-testkit-server)
 * - tddy-demo built with LiveKit support (cargo build -p tddy-demo)
 *
 * Flow: Cypress starts tddy-demo via startTerminalServer task, visits
 * GhosttyTerminal LiveKit story, connects, asserts RPC stream and buffer.
 *
 * Skipped when LIVEKIT_TESTKIT_WS_URL is not set.
 */
describe("Ghostty Terminal E2E", () => {
  let serverUrl: string;
  let clientToken: string;
  let roomName: string;

  let serverLogPath: string | undefined;

  before(function () {
    if (!Cypress.env("LIVEKIT_TESTKIT_WS_URL")) {
      this.skip();
      return;
    }
    return cy.task("startTerminalServer").then((result) => {
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

  after(function () {
    if (this.currentTest?.state === "failed" && serverLogPath) {
      cy.log(`Server debug log: ${serverLogPath}`);
    }
    cy.task("stopTerminalServer");
  });

  it("displays tddy-demo terminal output in ghostty through LiveKit", () => {
    const storyUrl = `/iframe.html?id=components-ghosttyterminal--live-kit-connected&url=${encodeURIComponent(serverUrl)}&token=${encodeURIComponent(clientToken)}&roomName=${encodeURIComponent(roomName)}`;
    cy.visit(storyUrl);

    cy.get("body", { timeout: 10000 }).should("be.visible");

    cy.get(
      "[data-testid='livekit-status'], [data-testid='livekit-placeholder'], [data-testid='livekit-error'], [data-testid='ghostty-terminal']",
      { timeout: 25000 }
    ).should("exist");

    cy.get("[data-testid='livekit-status']", { timeout: 10000 })
      .should("exist")
      .and("have.text", "connected");

    cy.get("[data-testid='ghostty-terminal']", { timeout: 5000 }).should(
      "exist"
    );

    cy.get("[data-testid='streamed-byte-count']", { timeout: 30000 }).should(
      ($el) => {
        const n = parseInt($el.text(), 10);
        expect(n).to.be.greaterThan(0, "RPC should have streamed bytes from tddy-demo");
      }
    );

    // Buffer receives streamed content (RPC + Ghostty pipeline working)
    cy.get("[data-testid='terminal-buffer-text']", { timeout: 20000 }).should(
      ($el) => {
        const raw = $el.text();
        expect(raw.length).to.be.greaterThan(0, "terminal buffer should have content from tddy-demo");
      }
    );
  });
});
