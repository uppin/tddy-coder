/**
 * E2E: App Connect flow — token fetch via Connect-RPC + LiveKit connection.
 *
 * Reproduces: "failed to decode Protobuf message: buffer underflow" when clicking
 * Connect (client was sending JSON, server expected protobuf).
 *
 * Requires: LIVEKIT_TESTKIT_WS_URL, tddy-coder built, web bundle built.
 * Skipped when LIVEKIT_TESTKIT_WS_URL is not set.
 */
import { appAuthPage } from "../support/pages/appAuthPage";

describe("App Connect Flow E2E", () => {
  let baseUrl: string;

  before(function () {
    cy.startTddyCoderApp({ flow: "connect" }).then((result) => {
      baseUrl = result.baseUrl;
    });
  });

  after(() => {
    cy.task("stopTddyCoderForConnectFlow");
  });

  it("connects via Connect-RPC token fetch without a buffer underflow error", () => {
    // Given
    const wsUrl = Cypress.env("LIVEKIT_TESTKIT_WS_URL") as string;
    cy.visit(baseUrl);

    // When — fill and submit the connection form
    appAuthPage.loginButton().should("exist").click();
    appAuthPage.livekitUrlInput({ timeout: 20000 }).should("exist").clear().type(wsUrl);
    appAuthPage.livekitIdentityInput().clear().type("client");
    appAuthPage.livekitRoomInput().clear().type("terminal-e2e");
    appAuthPage.submitButton().click();

    // Then — connection reaches "connected" without protobuf decode errors
    appAuthPage.statusDot({ timeout: 15000 }).should("be.visible").and("have.attr", "data-connection-status", "connected");
    appAuthPage.livekitUrlInput().should("not.be.visible");

    cy.get("[data-testid='livekit-error']").should("not.exist");
    cy.contains("buffer underflow").should("not.exist");
    cy.contains("failed to decode Protobuf").should("not.exist");
  });
});
