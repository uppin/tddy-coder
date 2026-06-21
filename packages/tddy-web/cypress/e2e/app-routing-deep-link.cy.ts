/**
 * E2E acceptance: real browser URL bar + full document load for terminal deep links.
 *
 * Requires: LIVEKIT_TESTKIT_WS_URL, built web bundle, tddy-coder (see cypress.config task).
 * Skipped when LIVEKIT_TESTKIT_WS_URL is not set.
 */
import { TERMINAL_SESSION_ROUTE_PREFIX } from "../../src/routing/appRoutes";
import { byTestId } from "../support/testIds";

describe("App routing — deep link reload (E2E)", () => {
  let baseUrl: string;

  before(function () {
    cy.startTddyCoderApp({ flow: "connect" }).then((result) => {
      baseUrl = result.baseUrl;
    });
  });

  after(() => {
    cy.task("stopTddyCoderForConnectFlow");
  });

  it("visiting a terminal deep link renders the connected terminal when the session is valid", () => {
    // Given
    const sid = "session-deep-link-e2e-1";

    // When
    cy.visit(`${baseUrl}${TERMINAL_SESSION_ROUTE_PREFIX}/${sid}`);

    // Then
    byTestId("connected-terminal-container", { timeout: 20000 }).should("exist");
    byTestId("routing-fatal-error").should("not.exist");
  });
});
