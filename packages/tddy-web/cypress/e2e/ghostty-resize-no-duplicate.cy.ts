/**
 * E2E: Terminal resize should not produce duplicate status bar content.
 *
 * Bug: when shrinking then growing the terminal, the TUI output shows duplicate
 * "PgUp/PgDn scroll" text instead of exactly once.
 *
 * Requires: LIVEKIT_TESTKIT_WS_URL, tddy-demo built.
 * Skipped when LIVEKIT_TESTKIT_WS_URL is not set.
 */
import type { TerminalSessionResult } from "../support/commands";

const ROWS_PX = 20;
const SHRINK_BY_ROWS = 10;
const GROW_BY_ROWS = 10;
const SHRINK_PX = ROWS_PX * SHRINK_BY_ROWS;
const GROW_PX = ROWS_PX * GROW_BY_ROWS;

const STORY_ID = "components-ghosttyterminal--live-kit-connected";

describe("Ghostty Resize No Duplicate", () => {
  let session: TerminalSessionResult = {} as TerminalSessionResult;

  before(function () {
    cy.startTerminalSession({ kind: "terminal", prompt: "Build auth" }).then((result) => {
      session = result;
    });
  });

  after(() => {
    cy.task("stopTerminalServer");
  });

  it("shows PgUp/PgDn scroll exactly once after a shrink-then-grow resize cycle", () => {
    // Given
    cy.visitGhosttyStory({ storyId: STORY_ID, url: session.url, token: session.clientToken, roomName: session.roomName });
    cy.connectAndWaitForTerminal();
    cy.waitForBufferText("PgUp/PgDn scroll");

    // When — simulate a resize cycle
    cy.viewport(1280, 720);
    // Brief settle after each viewport change to let Ghostty propagate the resize
    // eslint-disable-next-line cypress/no-unnecessary-waiting
    cy.wait(500); // justified: resize event + TUI re-render takes ~200-400ms
    cy.viewport(1280, 720 - SHRINK_PX);
    // eslint-disable-next-line cypress/no-unnecessary-waiting
    cy.wait(500);
    cy.viewport(1280, 720 - SHRINK_PX + GROW_PX);
    // eslint-disable-next-line cypress/no-unnecessary-waiting
    cy.wait(500);

    // Then — status bar appears exactly once — no duplicate panes
    cy.get("[data-testid='terminal-buffer-text']").should(($el) => {
      const matches = $el.text().match(/PgUp\/PgDn scroll/g);
      expect(
        matches?.length ?? 0,
        "PgUp/PgDn scroll should appear exactly once after resize",
      ).to.equal(1);
    });
  });
});
