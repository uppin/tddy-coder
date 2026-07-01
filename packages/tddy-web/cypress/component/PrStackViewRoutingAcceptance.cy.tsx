/**
 * Acceptance tests: per-workflow session views — the PR-Stack Chat Screen opens instead of the
 * terminal for "pr-stack" sessions, and every other session keeps the existing terminal view.
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views.
 * Changeset: docs/dev/1-WIP/pr-stack-workflow-views.md.
 *
 * All RPC calls flow through the in-memory backend — no HTTP intercepts.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function aSession(overrides: Record<string, unknown>) {
  return {
    createdAt: "2026-07-01T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    pid: 0,
    isActive: false,
    projectId: "proj-pr-stack",
    daemonInstanceId: "",
    workflowGoal: "",
    pendingElicitation: false,
    orchestratorSessionId: "",
    recipe: "",
    ...overrides,
  };
}

const PR_STACK_SESSION = aSession({
  sessionId: "pr-stack-session-0000-0000-0000-000000000001",
  recipe: "pr-stack",
});

const TDD_SESSION = aSession({
  sessionId: "tdd-session-0000-0000-0000-000000000002",
  recipe: "tdd",
});

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800); // desktop: session list defaults open so drawer items are clickable
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("opens the PR-Stack Chat Screen instead of the terminal for a pr-stack session", () => {
  // Given
  const backend = aSessionsDrawerBackend([PR_STACK_SESSION, TDD_SESSION]);

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then
  prStackScreenPage.screen().should("exist");
  sessionsDrawerPage.detailTerminalContainer().should("not.exist");
});

it("keeps the terminal placeholder for a session whose recipe is not pr-stack", () => {
  // Given
  const backend = aSessionsDrawerBackend([PR_STACK_SESSION, TDD_SESSION]);

  // When
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(TDD_SESSION.sessionId).click();

  // Then — the PR-Stack Chat Screen must not render for a non-pr-stack recipe; the existing
  // disconnected-terminal placeholder is unaffected by the new view registry.
  prStackScreenPage.screen().should("not.exist");
  sessionsDrawerPage.detailPane().should("contain.text", "Select Resume to reconnect");
});
