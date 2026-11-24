/**
 * Acceptance tests: per-workflow session views — every tddy-coder `tool` workflow session opens the
 * full-screen Workflow Chat Screen instead of the terminal.
 *
 * PRD: docs/ft/web/session-drawer.md § Per-Workflow Session Views.
 *
 * All RPC calls flow through the in-memory backend — no HTTP intercepts.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { CatalogService } from "tddy-client-web/gen/catalog_pb";
import { servedAgentWire, withDeclaredSessionTypes } from "../support/servedAgents";
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

/** The drawer's backend, serving the session types a daemon declares — `tool` among them, which is
 *  what makes a tddy-coder session's pane a workflow view. */
function aWorkflowSessionsBackend() {
  return aSessionsDrawerBackend([TDD_SESSION])
    .onUnary(CatalogService.method.listAgents, () =>
      withDeclaredSessionTypes({ agents: [{ id: "claude", label: "Claude", ...servedAgentWire("claude") }] }),
    );
}

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
  const backend = aWorkflowSessionsBackend();

  // When
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.selectEndedSession(TDD_SESSION.sessionId);

  // Then — the chat replaces the terminal, and it is full-screen: no planned-PR list pane.
  workflowChatScreenPage.screen().should("exist");
  workflowChatScreenPage.chat().should("exist");
  sessionsDrawerPage.detailTerminalContainer().should("not.exist");
  prStackScreenPage.workItemList().should("not.exist");
});
