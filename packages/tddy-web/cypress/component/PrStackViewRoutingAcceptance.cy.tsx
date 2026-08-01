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
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { workflowChatScreenPage } from "../support/pages/workflowChatScreenPage";
import { sessionActivitiesPage } from "../support/pages/sessionActivitiesPage";

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
    sessionType: "tool",
    ...overrides,
  };
}

const PR_STACK_SESSION = aSession({
  sessionId: "pr-stack-session-0000-0000-0000-000000000001",
  recipe: "pr-stack",
});

// A genuine Claude Code PTY session: it has no tddy-coder Presenter, so even though it carries a
// managed `recipe` it must keep the terminal — the routing gate keys on `sessionType`, not recipe.
const CLAUDE_CLI_SESSION = aSession({
  sessionId: "claude-cli-session-0000-0000-0000-000000000002",
  recipe: "tdd",
  sessionType: "claude-cli",
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
  const backend = aSessionsDrawerBackend([PR_STACK_SESSION, CLAUDE_CLI_SESSION]);

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then
  prStackScreenPage.screen().should("exist");
  sessionsDrawerPage.detailTerminalContainer().should("not.exist");
});

it("keeps the ordinary base view for a claude-cli session even when it carries a recipe", () => {
  // Given
  const backend = aSessionsDrawerBackend([PR_STACK_SESSION, CLAUDE_CLI_SESSION]);

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(CLAUDE_CLI_SESSION.sessionId).click();

  // Then — neither custom view renders for a claude-cli PTY session (it has no Presenter); the
  // session falls through to its liveness-derived base view, which for this dormant one is its
  // recorded activities. The view registry does not touch that path.
  prStackScreenPage.screen().should("not.exist");
  workflowChatScreenPage.screen().should("not.exist");
  sessionActivitiesPage.pane().should("exist");
});
