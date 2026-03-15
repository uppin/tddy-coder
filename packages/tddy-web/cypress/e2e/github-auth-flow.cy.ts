/**
 * E2E test: GitHub OAuth flow with stub provider.
 *
 * The server runs with --github-stub, which makes the authorize URL
 * redirect directly to /auth/callback on the same origin (no GitHub redirect).
 * This allows the full OAuth flow to be tested without external dependencies.
 *
 * Run via: bun run cypress:e2e:auth
 * Requires LIVEKIT_TESTKIT_WS_URL.
 */
describe("GitHub OAuth Flow E2E", () => {
  before(function () {
    if (!Cypress.env("LIVEKIT_TESTKIT_WS_URL")) {
      this.skip();
    }
  });

  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
  });

  it("shows login button when not authenticated", () => {
    cy.visit("/");
    cy.get("[data-testid='github-login-button']", { timeout: 10000 }).should("exist");
    cy.get("[data-testid='livekit-url']").should("not.exist");
  });

  it("completes login flow with stub and shows user info", () => {
    cy.visit("/");
    cy.get("[data-testid='github-login-button']", { timeout: 10000 }).should("exist");

    // Click login — the stub returns a callback URL on the same origin,
    // so the browser navigates directly to /auth/callback?code=test-code&state=...
    cy.get("[data-testid='github-login-button']").click();

    // Wait for callback page to load
    cy.url({ timeout: 10000 }).should("include", "/auth/callback");

    // Should see processing indicator, then redirect to /
    cy.get("body").then(($body) => {
      cy.log("Callback page body: " + $body.text().substring(0, 200));
    });

    // Wait for redirect back to / after successful auth
    cy.url({ timeout: 20000 }).should("not.include", "/auth/callback");

    // After callback processes and redirects to /, should show authenticated state
    cy.get("[data-testid='user-login']", { timeout: 15000 }).should(
      "have.text",
      "testuser"
    );
    cy.get("[data-testid='user-avatar']").should("exist");
    cy.get("[data-testid='logout-button']").should("exist");

    // Connection form should now be visible
    cy.get("[data-testid='livekit-url']").should("exist");
  });

  it("handles invalid code gracefully", () => {
    cy.visit("/auth/callback?code=invalid-code&state=bogus-state");
    cy.get("[data-testid='auth-error']", { timeout: 10000 }).should("exist");
  });

  it("logout returns to unauthenticated state", () => {
    // Login first by clicking login button
    cy.visit("/");
    cy.get("[data-testid='github-login-button']", { timeout: 10000 }).click();
    cy.get("[data-testid='user-login']", { timeout: 15000 }).should("exist");

    // Now logout
    cy.get("[data-testid='logout-button']").click();

    // Should show login button again
    cy.get("[data-testid='github-login-button']", { timeout: 10000 }).should("exist");
    cy.get("[data-testid='user-login']").should("not.exist");
  });
});
