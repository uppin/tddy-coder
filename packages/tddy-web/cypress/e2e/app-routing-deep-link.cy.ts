/**
 * E2E acceptance: real browser URL bar + full document load for terminal deep links.
 *
 * Requires LIVEKIT_TESTKIT_WS_URL, built web bundle, and tddy-coder (see cypress.config task).
 */
import { TERMINAL_SESSION_ROUTE_PREFIX } from "../../src/routing/appRoutes";

describe("App routing — deep link reload (E2E)", () => {
  let baseUrl: string;

  before(function () {
    if (!Cypress.env("LIVEKIT_TESTKIT_WS_URL")) {
      throw new Error(
        "visit_terminal_deep_link_renders_connected_terminal_when_session_valid requires LIVEKIT_TESTKIT_WS_URL (start ./run-livekit-testkit-server and export the URL).",
      );
    }
    return cy.task("startTddyCoderForConnectFlow").then((result) => {
      const r = result as { baseUrl: string };
      baseUrl = r.baseUrl;
    });
  });

  after(() => {
    cy.task("stopTddyCoderForConnectFlow");
  });

  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("visit_terminal_deep_link_renders_connected_terminal_when_session_valid", () => {
    const sid = "session-deep-link-e2e-1";
    cy.visit(`${baseUrl}${TERMINAL_SESSION_ROUTE_PREFIX}/${sid}`);
    cy.get("[data-testid='connected-terminal-container']", { timeout: 20000 }).should("exist");
    cy.get("[data-testid='routing-fatal-error']").should("not.exist");
  });
});
