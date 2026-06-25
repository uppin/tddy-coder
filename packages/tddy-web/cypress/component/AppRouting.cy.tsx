import React from "react";
import { App } from "../../src/index";
import { TERMINAL_SESSION_ROUTE_PREFIX } from "../../src/routing/appRoutes";
import {
  interceptConnectionRpcs,
  interceptConnectSession,
  interceptTokenForPresence,
} from "../support/rpc/connectionRpcs";
import { connectionPage } from "../support/pages/connectionPage";

// ---------------------------------------------------------------------------
// Test fixtures (session / project data — test data, not infrastructure)
// ---------------------------------------------------------------------------

const ACTIVE_SESSION = {
  sessionId: "session-active-1",
  createdAt: "2026-03-21T10:00:00Z",
  status: "active",
  repoPath: "/home/dev/project",
  pid: 12345,
  isActive: true,
  projectId: "proj-1",
};

const PROJECT_ID = "proj-1";

// ---------------------------------------------------------------------------
// Shared setup helpers
// ---------------------------------------------------------------------------

function interceptDaemonModeConfig() {
  cy.intercept("GET", "**/api/config", {
    statusCode: 200,
    headers: { "Content-Type": "application/json" },
    body: {
      daemon_mode: true,
      livekit_url: "ws://127.0.0.1:7880",
      common_room: "tddy-lobby",
    },
  }).as("apiConfigDaemon");
}

/** Wait until daemon ConnectionScreen has loaded (config + session list RPCs). */
function waitForDaemonSessionShell() {
  cy.contains("h3", "Projects", { timeout: 20000 }).should("be.visible");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe("App routing (daemon mode, acceptance)", () => {
  beforeEach(() => {
    cy.clearLocalStorage();
    cy.clearAllSessionStorage();
    interceptDaemonModeConfig();
  });

  it("connect keeps home URL and shows overlay; Expand pushes dedicated terminal route", () => {
    // Given — authenticated session with active session visible in the list
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptConnectionRpcs([ACTIVE_SESSION]);
    interceptConnectSession();
    interceptTokenForPresence();
    cy.window().then((win) => {
      win.location.hash = "/";
    });

    // When — mount, wait for session list, then click Connect
    cy.mount(<App />);
    waitForDaemonSessionShell();
    cy.wait("@getAuthStatus");
    connectionPage.connectBtn(ACTIVE_SESSION.sessionId, { timeout: 8000 }).click();
    cy.wait("@connectSession");

    // Then — URL stays at root (hash routing: pathname stays as Cypress frame path, hash stays #/);
    // reconnect overlay appears; Expand navigates to terminal route
    cy.window().its("location.hash").should("eq", "#/");
    connectionPage.reconnectOverlay().should("be.visible");
    connectionPage.reconnectExpand().should("be.visible").click();
    cy.window()
      .its("location.hash")
      .should("contain", `${TERMINAL_SESSION_ROUTE_PREFIX}/${ACTIVE_SESSION.sessionId}`);
  });

  it("navigating back from terminal returns to the session list", () => {
    // Given — connected and expanded to the dedicated terminal route
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptConnectionRpcs([ACTIVE_SESSION]);
    interceptConnectSession();
    interceptTokenForPresence();
    cy.window().then((win) => {
      win.location.hash = "/";
    });
    cy.mount(<App />);
    waitForDaemonSessionShell();
    cy.wait("@getAuthStatus");
    connectionPage.connectBtn(ACTIVE_SESSION.sessionId, { timeout: 8000 }).click();
    cy.wait("@connectSession");
    connectionPage.reconnectOverlay({ timeout: 8000 }).should("be.visible");
    connectionPage.reconnectExpand({ timeout: 8000 }).should("be.visible").click();
    connectionPage.terminalContainer({ timeout: 8000 }).should("exist");

    // When — browser back
    cy.window().then((win) => {
      win.history.back();
    });

    // Then — session list is shown; terminal is gone
    connectionPage.sessionsTable(PROJECT_ID, { timeout: 8000 }).should("be.visible");
    connectionPage.terminalContainer().should("not.exist");
  });

  it("navigating directly to an unknown terminal route shows an error with a home link", () => {
    // Given — hash points to a session ID that does not exist in the list
    window.localStorage.setItem("tddy_session_token", "fake-token");
    interceptConnectionRpcs([ACTIVE_SESSION]);
    interceptConnectSession();
    interceptTokenForPresence();
    cy.window().then((win) => {
      win.location.hash = `${TERMINAL_SESSION_ROUTE_PREFIX}/does-not-exist-999`;
    });

    // When — mount
    cy.mount(<App />);
    waitForDaemonSessionShell();
    cy.wait("@getAuthStatus");

    // Then — unknown-session error is shown; clicking home navigates back and clears the error
    connectionPage.unknownSessionError().should("be.visible");
    connectionPage.unknownSessionHomeLink().should("be.visible").click();
    // In Cypress component tests, pathname stays as the iframe path; hash routing uses #/
    cy.window().its("location.hash").should("eq", "#/");
    connectionPage.sessionsTable(PROJECT_ID, { timeout: 8000 }).should("exist");
    connectionPage.unknownSessionError().should("not.exist");
  });
});
