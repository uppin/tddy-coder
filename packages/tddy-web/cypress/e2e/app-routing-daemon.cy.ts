/**
 * E2E: App routing in daemon mode (real auth) — the sessions drawer is the default route.
 *
 * Full-`App` daemon-authenticated routing can't be exercised in the component-test harness
 * (its auth token gate needs a real server handshake), so it lives here against a real
 * tddy-daemon serving the built web bundle, authenticated via the GitHub stub.
 *
 * `#/sessions/:id` deep-link parsing + the unknown-session not-found state are covered by the
 * component spec `SessionsDrawerUnknownDeepLinkAcceptance.cy.tsx` (which seeds a session so the
 * list is non-empty); daemon-mode reload/reconnect is covered by `terminal-rendering.cy.ts`.
 *
 * Requires:
 *   cargo build -p tddy-demo-tui -p tddy-daemon
 *   bun run build   (web bundle in packages/tddy-web/dist)
 *   LIVEKIT_TESTKIT_WS_URL (or Docker for testcontainers)
 */

const DRAWER = "[data-testid='sessions-drawer-screen']";
const MENU_BTN = "[data-testid='shell-menu-button']";

describe("App routing (daemon mode) E2E", () => {
  let baseUrl: string;

  before(function () {
    if (!Cypress.env("LIVEKIT_TESTKIT_WS_URL")) {
      this.skip();
    }
    cy.task("startDaemonWithDemoTui").then((result) => {
      baseUrl = (result as { baseUrl: string }).baseUrl;
    });
  });

  after(() => {
    cy.task("stopDaemonWithDemoTui");
  });

  /** Visit `path` authenticated (stub OAuth token seeded into localStorage before load). */
  function visitAuthed(path: string) {
    cy.task("getTestSessionToken", { baseUrl }).then((token) => {
      cy.visit(`${baseUrl}${path}`, {
        onBeforeLoad(win) {
          win.localStorage.setItem("tddy_session_token", token as string);
        },
      });
    });
  }

  it("renders the sessions drawer with the navigation menu at the default route", () => {
    // Given / When — the authenticated app is opened at the root route
    visitAuthed("/#/");

    // Then — the sessions drawer is the default view, carrying the unified hamburger menu
    cy.get(DRAWER, { timeout: 10000 }).should("exist");
    cy.get(MENU_BTN, { timeout: 10000 }).should("be.visible");
  });
});
