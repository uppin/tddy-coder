/**
 * E2E: Terminal rendering — initial column width, no overflow, resize, reconnect.
 *
 * Uses tddy-daemon with tddy-demo-tui as the claude_cli binary.
 * tddy-demo-tui draws "DEMO TUI W={cols} H={rows}" on a clear screen and redraws on SIGWINCH,
 * making column width directly observable from terminal text content.
 *
 * Requires:
 *   cargo build -p tddy-demo-tui -p tddy-daemon
 *   bun run build   (web bundle in packages/tddy-web/dist)
 */

const TERMINAL_BUFFER = "[data-testid='terminal-buffer-text']";
const NEW_SESSION_BTN = "[data-testid='sessions-drawer-new-btn']";
const CREATE_CLAUDE_CLI_BTN = "[data-testid='create-session-type-claude-cli']";
const SUBMIT_BTN = "[data-testid='create-session-submit-btn']";
const TERMINAL_CONTAINER = "[data-testid='sessions-detail-terminal-container']";

const TERMINAL_READY_TIMEOUT = 30000;
// Time to let a SIGWINCH-triggered redraw propagate to the browser after a resize.
const RESIZE_SETTLE_MS = 800;

describe("Terminal rendering — gRPC transport with demo TUI", () => {
  let baseUrl: string;

  before(function () {
    cy.task("startDaemonWithDemoTui").then((result) => {
      baseUrl = (result as { baseUrl: string }).baseUrl;
    });
  });

  after(() => {
    cy.task("stopDaemonWithDemoTui");
  });

  /** Authenticate and open the sessions drawer. */
  function visitSessionsDrawer() {
    cy.task("getTestSessionToken", { baseUrl }).then((token) => {
      cy.visit(`${baseUrl}/#/sessions`, {
        onBeforeLoad(win) {
          win.localStorage.setItem("tddy_session_token", token as string);
        },
      });
    });
    cy.get("[data-testid='sessions-drawer-screen']", { timeout: 10000 }).should("exist");
  }

  /** Create a Claude CLI session and wait for the demo TUI to draw its first frame. */
  function createClaudeCliSession() {
    cy.get(NEW_SESSION_BTN, { timeout: 5000 }).click();
    cy.get(CREATE_CLAUDE_CLI_BTN, { timeout: 5000 }).click();
    // Wait for the project to auto-load (makes the submit button enabled).
    cy.get(SUBMIT_BTN, { timeout: 10000 }).should("not.be.disabled");
    // The daemon requires a branch name for new_branch_from_base intent.
    cy.get("[data-testid='create-session-new-branch-name-input']").type("e2e-test");
    cy.get(SUBMIT_BTN).click();
    cy.get(TERMINAL_CONTAINER, { timeout: 15000 }).should("exist");
    cy.get(TERMINAL_BUFFER, { timeout: TERMINAL_READY_TIMEOUT }).should(
      "contain",
      "DEMO TUI W=",
    );
  }

  it("AC1: initial render shows the actual container width — not the PTY default (220)", () => {
    visitSessionsDrawer();
    createClaudeCliSession();

    cy.get(TERMINAL_BUFFER).should(($el) => {
      const text = $el.text();
      const match = text.match(/DEMO TUI W=(\d+)/);
      expect(match, "terminal must contain DEMO TUI W=<cols>").to.not.be.null;
      const cols = parseInt(match![1], 10);
      expect(cols, `cols (${cols}) must not be the PTY default 220`).to.not.equal(220);
      expect(cols, `cols (${cols}) must be a plausible terminal width`).to.be.above(40);
    });
  });

  it("AC2: initial render has no horizontal overflow in the terminal container", () => {
    visitSessionsDrawer();
    createClaudeCliSession();

    cy.get(TERMINAL_CONTAINER).should(($el) => {
      const el = $el[0];
      expect(
        el.scrollWidth,
        `scrollWidth (${el.scrollWidth}) must not exceed offsetWidth (${el.offsetWidth})`,
      ).to.be.at.most(el.offsetWidth + 2); // +2 tolerates subpixel rounding
    });
  });

  it("AC3: resizing the viewport causes the terminal to redraw with new column count", () => {
    visitSessionsDrawer();
    createClaudeCliSession();

    let initialCols = 0;
    cy.get(TERMINAL_BUFFER).should(($el) => {
      const match = $el.text().match(/DEMO TUI W=(\d+)/);
      expect(match, "initial terminal text must contain W=<cols>").to.not.be.null;
      initialCols = parseInt(match![1], 10);
    });

    // Shrink the viewport width significantly.
    cy.viewport(800, 600);
    // eslint-disable-next-line cypress/no-unnecessary-waiting
    cy.wait(RESIZE_SETTLE_MS); // justified: SIGWINCH → demo-tui redraw → broadcast → browser

    cy.get(TERMINAL_BUFFER).should(($el) => {
      const match = $el.text().match(/DEMO TUI W=(\d+)/);
      expect(match, "terminal must contain W=<cols> after resize").to.not.be.null;
      const newCols = parseInt(match![1], 10);
      expect(
        newCols,
        `cols after resize (${newCols}) must differ from pre-resize (${initialCols})`,
      ).to.not.equal(initialCols);
      expect(newCols, "cols after resize must be > 0").to.be.above(0);
    });
  });

  it("AC4: reconnecting after a resize shows correct width immediately — no 220-col flash", () => {
    visitSessionsDrawer();
    createClaudeCliSession();

    let colsAfterConnect = 0;
    cy.get(TERMINAL_BUFFER).should(($el) => {
      const match = $el.text().match(/DEMO TUI W=(\d+)/);
      expect(match, "initial terminal text must contain W=<cols>").to.not.be.null;
      colsAfterConnect = parseInt(match![1], 10);
    });

    // Reconnect: reload the page while keeping the session token.
    cy.task("getTestSessionToken", { baseUrl }).then((token) => {
      cy.visit(`${baseUrl}/#/sessions`, {
        onBeforeLoad(win) {
          win.localStorage.setItem("tddy_session_token", token as string);
        },
      });
    });
    cy.get("[data-testid='sessions-drawer-screen']", { timeout: 10000 }).should("exist");

    // The session that was created above should still be in the list and active.
    // Click the first session in the drawer to reconnect.
    cy.get("[data-testid^='sessions-drawer-item-']", { timeout: 10000 }).first().click();
    cy.get(TERMINAL_BUFFER, { timeout: TERMINAL_READY_TIMEOUT }).should(
      "contain",
      "DEMO TUI W=",
    );

    cy.get(TERMINAL_BUFFER).should(($el) => {
      const text = $el.text();
      const match = text.match(/DEMO TUI W=(\d+)/);
      expect(match, "terminal on reconnect must contain DEMO TUI W=<cols>").to.not.be.null;
      const cols = parseInt(match![1], 10);
      expect(
        cols,
        `cols on reconnect (${cols}) must not be the PTY default 220`,
      ).to.not.equal(220);
      expect(
        cols,
        `cols on reconnect (${cols}) must be close to initial connection (${colsAfterConnect})`,
      ).to.be.within(colsAfterConnect - 20, colsAfterConnect + 20);
    });
  });
});
