/**
 * E2E: Fullscreen terminal should not show duplicate question+status+prompt panes.
 *
 * Bug: When on fullscreen, the terminal shows 2 copies of the status bar; only
 * the bottom one accepts input. The upper one is stale content from scrollback.
 *
 * Root cause: xterm.js/ghostty-web defaults to scrollback > 0. When the backend
 * streams multiple frames, old frames accumulate in scrollback and appear above
 * the current viewport.
 *
 * This test:
 * 1. Connects to tddy-demo via LiveKit (same as ghostty-selection-persistence)
 * 2. Waits for the scope Select question to appear
 * 3. Waits for periodic re-renders to stream multiple frames
 * 4. Asserts the status bar (`Goal:`) appears only once in the buffer (no duplicate panes)
 *
 * Requires: LIVEKIT_TESTKIT_WS_URL, tddy-demo built
 */
describe("Ghostty Fullscreen No Duplicate", () => {
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

  it("shows only one status bar when streaming multiple frames (no duplicate panes)", () => {
    const storyUrl = `/iframe.html?id=components-ghosttyterminal--live-kit-connected&url=${encodeURIComponent(serverUrl)}&token=${encodeURIComponent(clientToken)}&roomName=${encodeURIComponent(roomName)}`;
    cy.visit(storyUrl);

    cy.get("[data-testid='connection-status-dot']", { timeout: 15000 })
      .should("be.visible")
      .and("have.attr", "data-connection-status", "connected");
    cy.get("[data-testid='livekit-status']").should("not.be.visible");

    cy.get("[data-testid='ghostty-terminal']", { timeout: 5000 }).should("exist");

    cy.get("[data-testid='first-output-received']", { timeout: 15000 }).should("exist");

    cy.get("[data-testid='terminal-buffer-text']", { timeout: 20000 }).should(
      ($el) => {
        const text = $el.text();
        expect(text).to.include("Email/password");
      }
    );

    cy.wait(2000);

    cy.get("[data-testid='terminal-buffer-text']").should(($el) => {
      const text = $el.text();
      // TUI status line always contains `Goal:` (see tddy-tui format_status_bar). Duplicate panes
      // duplicate the whole bar, so counting `Goal:` catches the regression.
      const matches = text.match(/Goal:/g);
      expect(
        matches?.length ?? 0,
        "status bar should appear only once (no duplicate panes from scrollback)"
      ).to.equal(1);
    });
  });
});
