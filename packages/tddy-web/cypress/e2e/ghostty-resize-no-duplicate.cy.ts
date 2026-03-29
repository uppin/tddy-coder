/**
 * E2E: Terminal resize should not produce duplicate status bar content.
 *
 * Bug: When shrinking and growing the terminal, the TUI output shows duplicate
 * "PgUp/PgDn scroll" (or other status bar content) instead of exactly once.
 *
 * This test:
 * 1. Connects to tddy-demo via LiveKit
 * 2. Waits for first output
 * 3. Shrinks viewport by ~10 rows, waits for resize to propagate
 * 4. Grows viewport by ~10 rows, waits for resize to propagate
 * 5. Asserts "PgUp/PgDn scroll" appears exactly once in the buffer
 *
 * Requires: LIVEKIT_TESTKIT_WS_URL, tddy-demo built
 */

const ROWS_PX = 20;
const SHRINK_BY_ROWS = 10;
const GROW_BY_ROWS = 10;
const SHRINK_PX = ROWS_PX * SHRINK_BY_ROWS;
const GROW_PX = ROWS_PX * GROW_BY_ROWS;

describe("Ghostty Resize No Duplicate", () => {
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

  it("shows PgUp/PgDn scroll exactly once after shrink and grow by 10 rows", () => {
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
        expect(text).to.include("PgUp/PgDn scroll");
      }
    );

    cy.viewport(1280, 720);
    cy.wait(500);

    cy.viewport(1280, 720 - SHRINK_PX);
    cy.wait(500);

    cy.viewport(1280, 720 - SHRINK_PX + GROW_PX);
    cy.wait(500);

    cy.get("[data-testid='terminal-buffer-text']").should(($el) => {
      const text = $el.text();
      const matches = text.match(/PgUp\/PgDn scroll/g);
      expect(
        matches?.length ?? 0,
        "PgUp/PgDn scroll should appear exactly once after resize"
      ).to.equal(1);
    });
  });
});
