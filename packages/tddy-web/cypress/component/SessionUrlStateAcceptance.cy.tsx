/**
 * Acceptance tests: navigating between sessions updates the URL, and the URL is the source of
 * truth for the selection — Back steps through the selection trail, and an inbound hash change
 * (edited address bar, pasted link) moves the selection without a reload.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-08-01-url-state-routing.md.
 * Changeset: docs/dev/1-WIP/2026-08-01-web-url-state-routing.md.
 *
 * All RPC calls flow through the in-memory backend — no HTTP intercepts.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { appLocationPage } from "../support/pages/appLocationPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function aSession(sessionId: string, workflowGoal: string) {
  return {
    sessionId,
    createdAt: "2026-08-01T09:00:00Z",
    status: "exited",
    repoPath: "/home/dev/url-state",
    pid: 0,
    isActive: false,
    projectId: "proj-url-state",
    daemonInstanceId: "",
    workflowGoal,
    pendingElicitation: false,
    orchestratorSessionId: "",
    recipe: "",
    sessionType: "claude-cli",
  };
}

const ALPHA = aSession("alpha-0000-0000-0000-000000000001", "Alpha session");
const BRAVO = aSession("bravo-0000-0000-0000-000000000002", "Bravo session");
const CHARLIE = aSession("charlie-0000-0000-0000-000000000003", "Charlie session");

const ALL_SESSIONS = [ALPHA, BRAVO, CHARLIE];

/** Mount the sessions drawer over an in-memory backend holding all three fixture sessions. */
function mountSessionsDrawer() {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), aSessionsDrawerBackend(ALL_SESSIONS));
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800); // desktop: the session list defaults open so drawer rows are clickable
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
  appLocationPage.reset();
});

// ---------------------------------------------------------------------------
// Writing the URL
// ---------------------------------------------------------------------------

it("selecting a session in the drawer puts its id in the URL", () => {
  // Given
  mountSessionsDrawer();

  // When
  sessionsDrawerPage.drawerItem(BRAVO.sessionId).click();

  // Then
  appLocationPage.expectPath(`/sessions/${BRAVO.sessionId}`);
});

it("selecting a different session replaces the id in the URL", () => {
  // Given
  mountSessionsDrawer();
  sessionsDrawerPage.drawerItem(ALPHA.sessionId).click();

  // When
  sessionsDrawerPage.drawerItem(CHARLIE.sessionId).click();

  // Then
  appLocationPage.expectPath(`/sessions/${CHARLIE.sessionId}`);
});

it("selecting a session keeps the host the URL already named", () => {
  // Given — a deep link that pins the host
  appLocationPage.startAt("/sessions?host=local");
  mountSessionsDrawer();

  // When
  sessionsDrawerPage.drawerItem(BRAVO.sessionId).click();

  // Then — the session moved, the host did not
  appLocationPage.expectPath(`/sessions/${BRAVO.sessionId}`);
  appLocationPage.expectParam("host", "local");
});

// ---------------------------------------------------------------------------
// Reading the URL — Back / Forward
// ---------------------------------------------------------------------------

it("going back after selecting a second session re-selects the first", () => {
  // Given — the operator clicked through two sessions
  mountSessionsDrawer();
  sessionsDrawerPage.drawerItem(ALPHA.sessionId).click();
  appLocationPage.expectPath(`/sessions/${ALPHA.sessionId}`);
  sessionsDrawerPage.drawerItem(BRAVO.sessionId).click();
  appLocationPage.expectPath(`/sessions/${BRAVO.sessionId}`);

  // When
  appLocationPage.goBack();

  // Then — the URL and the drawer both return to the first session
  appLocationPage.expectPath(`/sessions/${ALPHA.sessionId}`);
  sessionsDrawerPage.expectSelected(ALPHA.sessionId);
  sessionsDrawerPage.expectNotSelected(BRAVO.sessionId);
});

it("going forward after going back re-selects the second session", () => {
  // Given
  mountSessionsDrawer();
  sessionsDrawerPage.drawerItem(ALPHA.sessionId).click();
  sessionsDrawerPage.drawerItem(BRAVO.sessionId).click();
  appLocationPage.goBack();
  sessionsDrawerPage.expectSelected(ALPHA.sessionId);

  // When
  appLocationPage.goForward();

  // Then
  appLocationPage.expectPath(`/sessions/${BRAVO.sessionId}`);
  sessionsDrawerPage.expectSelected(BRAVO.sessionId);
});

// ---------------------------------------------------------------------------
// Reading the URL — inbound changes and deep links
// ---------------------------------------------------------------------------

it("an inbound hash change selects the named session without a reload", () => {
  // Given — the operator is looking at one session
  mountSessionsDrawer();
  sessionsDrawerPage.drawerItem(ALPHA.sessionId).click();
  sessionsDrawerPage.expectSelected(ALPHA.sessionId);

  // When — a link to another session is pasted into the open tab
  appLocationPage.navigateExternally(`/sessions/${CHARLIE.sessionId}`);

  // Then — the mounted screen follows it
  sessionsDrawerPage.expectSelected(CHARLIE.sessionId);
  sessionsDrawerPage.expectNotSelected(ALPHA.sessionId);
});

it("a #/sessions/:id deep link selects that session on load", () => {
  // Given
  appLocationPage.startAt(`/sessions/${CHARLIE.sessionId}`);

  // When
  mountSessionsDrawer();

  // Then
  sessionsDrawerPage.expectSelected(CHARLIE.sessionId);
});

// ---------------------------------------------------------------------------
// Create-session pane
// ---------------------------------------------------------------------------

it("opening the create-session pane navigates to /sessions/new", () => {
  // Given
  mountSessionsDrawer();

  // When
  sessionsDrawerPage.newSessionBtn().click();

  // Then
  appLocationPage.expectPath("/sessions/new");
  sessionsDrawerPage.createSessionPane().should("exist");
});

it("a /sessions/new deep link opens the create-session pane on load", () => {
  // Given
  appLocationPage.startAt("/sessions/new");

  // When
  mountSessionsDrawer();

  // Then
  sessionsDrawerPage.createSessionPane().should("exist");
});

it("going back from the create-session pane returns to the previously selected session", () => {
  // Given — a session was selected, then the create pane was opened
  mountSessionsDrawer();
  sessionsDrawerPage.drawerItem(ALPHA.sessionId).click();
  sessionsDrawerPage.newSessionBtn().click();
  appLocationPage.expectPath("/sessions/new");

  // When
  appLocationPage.goBack();

  // Then
  appLocationPage.expectPath(`/sessions/${ALPHA.sessionId}`);
  sessionsDrawerPage.createSessionPane().should("not.exist");
  sessionsDrawerPage.expectSelected(ALPHA.sessionId);
});
