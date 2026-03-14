/**
 * E2E test: App Connect flow — token fetch via Connect-RPC, LiveKit connection.
 *
 * Reproduces: "failed to decode Protobuf message: buffer underflow" when
 * clicking Connect (client sends JSON, server expects protobuf).
 *
 * Requires:
 * - LIVEKIT_TESTKIT_WS_URL (from ./run-livekit-testkit-server)
 * - tddy-coder built (cargo build -p tddy-coder)
 * - Web bundle built (bun run build)
 *
 * Flow: Cypress starts tddy-coder with API key/secret and web bundle,
 * visits the app, fills form, clicks Connect. Asserts token fetch succeeds
 * and connection reaches "connected" (no buffer underflow error).
 *
 * Skipped when LIVEKIT_TESTKIT_WS_URL is not set.
 */
describe("App Connect Flow E2E", () => {
  let baseUrl: string;

  before(function () {
    if (!Cypress.env("LIVEKIT_TESTKIT_WS_URL")) {
      this.skip();
      return;
    }
    return cy.task("startTddyCoderForConnectFlow").then((result) => {
      const r = result as { baseUrl: string };
      baseUrl = r.baseUrl;
    });
  });

  after(() => {
    cy.task("stopTddyCoderForConnectFlow");
  });

  it("connects via Connect-RPC token fetch without buffer underflow error", () => {
    const wsUrl = Cypress.env("LIVEKIT_TESTKIT_WS_URL") as string;
    cy.visit(baseUrl);

    cy.get("#livekit-url", { timeout: 10000 }).should("exist").clear().type(wsUrl);
    cy.get("[data-testid='livekit-identity']").clear().type("client");
    cy.get("#livekit-room").clear().type("terminal-e2e");
    cy.get("button[type='submit']").click();

    cy.get("[data-testid='livekit-status']", { timeout: 15000 })
      .should("exist")
      .and("have.text", "connected");

    cy.get("[data-testid='livekit-error']").should("not.exist");
    cy.contains("buffer underflow").should("not.exist");
    cy.contains("failed to decode Protobuf").should("not.exist");
  });
});
