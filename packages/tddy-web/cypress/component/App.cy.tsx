import React from "react";
import { App } from "../../src/index";
import {
  anAuthStatusAuthenticated,
  anAuthStatusUnauthenticated,
  aGenerateTokenResponse,
  aRefreshTokenResponse,
} from "../support/rpc/responses";
import { toArrayBuffer } from "../support/rpc/protoRpc";
import { byTestId, TEST_IDS } from "../support/testIds";
import { connectionPage } from "../support/pages/connectionPage";

// Mobile tap-to-type / focus assertions need a real LiveKit room (server participant). Those flows are covered in GhosttyTerminalLiveKit.cy.tsx without full App wiring.
describe("App", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    // App gates on GET /api/config (vite proxies to DAEMON_PORT). A real daemon may return
    // daemon_mode: true → ConnectionScreen without #livekit-url. Force standalone form for CT.
    cy.intercept("GET", "**/api/config", {
      statusCode: 200,
      headers: { "Content-Type": "application/json" },
      body: { daemon_mode: false },
    }).as("apiConfig");
  });

  it("shows login button when not authenticated", () => {
    // Given — unauthenticated auth status
    const body = toArrayBuffer(anAuthStatusUnauthenticated());
    cy.intercept("POST", "**/rpc/auth.AuthService/GetAuthStatus", (req) => {
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
    }).as("getAuthStatus");

    // When
    cy.mount(<App />);

    // Then
    byTestId(TEST_IDS.githubLoginButton, { timeout: 5000 }).should("exist");
    cy.get("#livekit-url").should("not.exist");
  });

  it("shows identity, url, and roomName form fields when authenticated", () => {
    // Given — authenticated session
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const body = toArrayBuffer(anAuthStatusAuthenticated());
    cy.intercept("POST", "**/rpc/auth.AuthService/GetAuthStatus", (req) => {
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body });
    }).as("getAuthStatus");

    // When
    cy.mount(<App />);
    cy.wait("@getAuthStatus");

    // Then
    cy.get("#livekit-url", { timeout: 5000 }).should("exist");
    byTestId(TEST_IDS.livekitIdentity).should("exist");
    cy.get("#livekit-room").should("exist");
    byTestId(TEST_IDS.userLogin).should("have.text", "testuser");
  });

  it("connects via Connect-RPC token fetch when authenticated and form submitted", () => {
    // Given — authenticated session with token intercepts
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const authBody = toArrayBuffer(anAuthStatusAuthenticated());
    cy.intercept("POST", "**/rpc/auth.AuthService/GetAuthStatus", (req) => {
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: authBody });
    }).as("getAuthStatus");

    const generateBody = toArrayBuffer(aGenerateTokenResponse({ token: "mock-jwt-from-rpc", ttlSeconds: 600n }));
    cy.intercept("POST", "**/rpc/token.TokenService/GenerateToken", (req) => {
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: generateBody });
    }).as("generateToken");

    const refreshBody = toArrayBuffer(aRefreshTokenResponse({ token: "mock-jwt-from-rpc", ttlSeconds: 600n }));
    cy.intercept("POST", "**/rpc/token.TokenService/RefreshToken", (req) => {
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: refreshBody });
    }).as("refreshToken");

    // When — mount and submit the connection form
    cy.mount(<App />);
    cy.wait("@getAuthStatus");
    cy.get("#livekit-url", { timeout: 5000 }).type("ws://localhost:7880");
    byTestId(TEST_IDS.livekitIdentity).type("client");
    cy.get("#livekit-room").clear().type("terminal-e2e");
    cy.get("button[type='submit']").click();
    cy.wait("@generateToken");

    // Then — connection chrome is visible; form is gone; terminal container is fullscreen
    // Token fetch phase: primary status is the connection chrome (dot), not text-only livekit-status alone.
    connectionPage.statusDot({ timeout: 5000 }).should("exist");
    cy.get("#livekit-url").should("not.exist");

    // Acceptance: connection chrome — top-right status dot (with state); interrupt is TUI Stop pane (not web Stop).
    connectionPage.statusDot().should("have.attr", "data-connection-status");
    byTestId(TEST_IDS.ctrlCButton).should("not.exist");

    // Acceptance: connected terminal container is fullscreen (100vw x 100vh)
    connectionPage.terminalContainer({ timeout: 5000 })
      .should("exist")
      .then(($el) => {
        const rect = $el[0].getBoundingClientRect();
        expect(rect.width).to.be.greaterThan(0);
        expect(rect.height).to.be.greaterThan(0);
        // Fullscreen: container should fill viewport (allow small tolerance)
        expect(rect.width).to.equal(Cypress.config("viewportWidth"));
        expect(rect.height).to.equal(Cypress.config("viewportHeight"));
      });
  });

  it("shows mobile keyboard button when connected on touch-capable device", () => {
    // Given — mobile viewport and authenticated session with token intercepts
    cy.viewport(375, 667);
    window.localStorage.setItem("tddy_session_token", "fake-token");
    const authBody = toArrayBuffer(anAuthStatusAuthenticated());
    cy.intercept("POST", "**/rpc/auth.AuthService/GetAuthStatus", (req) => {
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: authBody });
    }).as("getAuthStatus");

    const generateBody = toArrayBuffer(aGenerateTokenResponse({ token: "mock-jwt-from-rpc", ttlSeconds: 600n }));
    cy.intercept("POST", "**/rpc/token.TokenService/GenerateToken", (req) => {
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: generateBody });
    }).as("generateToken");

    const refreshBody = toArrayBuffer(aRefreshTokenResponse({ token: "mock-jwt-from-rpc", ttlSeconds: 600n }));
    cy.intercept("POST", "**/rpc/token.TokenService/RefreshToken", (req) => {
      req.reply({ statusCode: 200, headers: { "Content-Type": "application/proto" }, body: refreshBody });
    }).as("refreshToken");

    // When — mount and submit the connection form
    cy.mount(<App />);
    cy.wait("@getAuthStatus");
    cy.get("#livekit-url", { timeout: 5000 }).type("ws://localhost:7880");
    byTestId(TEST_IDS.livekitIdentity).type("client");
    cy.get("#livekit-room").clear().type("terminal-e2e");
    cy.get("button[type='submit']").click();
    cy.wait("@generateToken");

    // Then — on mobile (touch-capable), keyboard button appears at bottom when keyboard closed
    connectionPage.terminalContainer({ timeout: 5000 }).should("exist");
    byTestId(TEST_IDS.buildId, { timeout: 2000 }).should("exist");
    byTestId(TEST_IDS.mobileKeyboardButton, { timeout: 5000 }).should("exist");
  });
});
