/**
 * Acceptance tests: per-workflow session views — every non-"pr-stack" tddy-coder `tool` workflow
 * session opens the full-screen Workflow Chat Screen instead of the terminal, while `pr-stack` keeps
 * its own two-pane screen and non-tool (claude-cli / cursor-cli) sessions keep the terminal.
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views.
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

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

function aSession(overrides: Record<string, unknown>) {
  return {
    createdAt: "2026-07-01T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/workflow-chat-project",
    pid: 0,
    isActive: false,
    projectId: "proj-workflow-chat",
    daemonInstanceId: "",
    workflowGoal: "",
    pendingElicitation: false,
    orchestratorSessionId: "",
    recipe: "",
    sessionType: "tool",
    ...overrides,
  };
}

const TDD_SESSION = aSession({
  sessionId: "tdd-session-0000-0000-0000-000000000001",
  recipe: "tdd",
});

const PR_STACK_SESSION = aSession({
  sessionId: "pr-stack-session-0000-0000-0000-000000000002",
  recipe: "pr-stack",
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

it("opens the full-screen Workflow Chat Screen instead of the terminal for a tool workflow session", () => {
  // Given
  const backend = aSessionsDrawerBackend([TDD_SESSION, PR_STACK_SESSION]);

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(TDD_SESSION.sessionId).click();

  // Then — the chat replaces the terminal, and it is full-screen: no pr-stack planned-PR list pane.
  workflowChatScreenPage.screen().should("exist");
  workflowChatScreenPage.chat().should("exist");
  sessionsDrawerPage.detailTerminalContainer().should("not.exist");
  prStackScreenPage.plannedPrList().should("not.exist");
  prStackScreenPage.screen().should("not.exist");
});

it("keeps the two-pane PR-Stack Chat Screen for a pr-stack session (not the full-screen chat)", () => {
  // Given
  const backend = aSessionsDrawerBackend([TDD_SESSION, PR_STACK_SESSION]);

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(PR_STACK_SESSION.sessionId).click();

  // Then — pr-stack still routes to its own dedicated screen, never the generic full-screen chat.
  prStackScreenPage.screen().should("exist");
  prStackScreenPage.plannedPrList().should("exist");
  workflowChatScreenPage.screen().should("not.exist");
});
