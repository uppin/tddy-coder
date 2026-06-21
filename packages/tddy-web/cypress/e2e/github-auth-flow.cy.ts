/**
 * E2E: GitHub OAuth flow with stub provider.
 *
 * The server runs with --github-stub, which makes the authorize URL redirect
 * directly to /auth/callback on the same origin — no real GitHub involved.
 *
 * Run via: bun run cypress:e2e:auth
 * Requires: LIVEKIT_TESTKIT_WS_URL.
 * Skipped when LIVEKIT_TESTKIT_WS_URL is not set.
 */
import { appAuthPage } from "../support/pages/appAuthPage";
import { byTestId } from "../support/testIds";

describe("GitHub OAuth Flow E2E", () => {
  before(function () {
    if (!Cypress.env("LIVEKIT_TESTKIT_WS_URL")) {
      this.skip();
    }
  });

  it("shows the login button when not authenticated", () => {
    // Given / When
    cy.visit("/");

    // Then
    appAuthPage.loginButton().should("exist");
    appAuthPage.livekitUrlInput().should("not.exist");
  });

  it("completes the login flow with the stub provider and shows user info", () => {
    // Given
    cy.visit("/");
    appAuthPage.loginButton().should("exist");

    // When — click login (stub returns a callback URL on the same origin)
    appAuthPage.loginButton().click();

    // Then — browser navigates to /auth/callback
    cy.url({ timeout: 10000 }).should("include", "/auth/callback");
    cy.get("body").then(($body) => {
      cy.log("Callback page body: " + $body.text().substring(0, 200));
    });

    // Then — redirects back to / after successful auth
    cy.url({ timeout: 20000 }).should("not.include", "/auth/callback");

    // Then — authenticated state with user info visible
    appAuthPage.userLogin({ timeout: 15000 }).should("have.text", "testuser");
    byTestId("user-avatar").should("exist");
    byTestId("logout-button").should("exist");
    appAuthPage.livekitUrlInput().should("exist");
  });

  it("shows an error page for an invalid OAuth callback code", () => {
    // Given / When
    cy.visit("/auth/callback?code=invalid-code&state=bogus-state");

    // Then
    byTestId("auth-error", { timeout: 10000 }).should("exist");
  });

  it("returns to unauthenticated state after logout", () => {
    // Given — log in first
    cy.visit("/");
    appAuthPage.loginButton({ timeout: 10000 }).click();
    appAuthPage.userLogin({ timeout: 15000 }).should("exist");

    // When
    byTestId("logout-button").click();

    // Then
    appAuthPage.loginButton({ timeout: 10000 }).should("exist");
    appAuthPage.userLogin().should("not.exist");
  });
});
