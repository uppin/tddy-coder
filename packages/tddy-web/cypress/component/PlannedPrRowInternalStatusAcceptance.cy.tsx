/**
 * Acceptance tests: the internal-status badge on a planned-PR row.
 *
 * Each node carries an optional `internal_status` (the action-needed signal, distinct from the
 * GitHub-reality `pr_status`). The row renders it as a badge next to the phase chip, labelled by
 * `internal_status.kind`, with `internal_status.note` as hover text.
 *
 * PRD: docs/ft/coder/pr-stacking.md § Internal PR status. Changeset:
 * docs/dev/1-WIP/pr-stack-free-prompting-loop.md.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { AuthProvider } from "../../src/hooks/authProvider";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { aPlannedNode, aStackPlanJson } from "../support/rpc/prStackFixtures";

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-2222-0000-0000-0000-000000000020";

function anOrchestratorSession(stackPlanJson: string) {
  return {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-07-03T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    pid: 0,
    isActive: false,
    projectId: "proj-pr-stack",
    daemonInstanceId: "",
    workflowGoal: "",
    pendingElicitation: false,
    orchestratorSessionId: "",
    recipe: "pr-stack",
    stackPlanJson,
  };
}

function openPrStackScreen(session: ReturnType<typeof anOrchestratorSession>) {
  const backend = aSessionsDrawerBackend([session]);
  mountWithRpc(
    <AuthProvider>
      <SessionsDrawerScreen />
    </AuthProvider>,
    backend,
  );
  sessionsDrawerPage.drawerItem(session.sessionId).click();
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

it("renders a needs-repoint badge on a node whose parent has merged", () => {
  // Given
  const plan = aStackPlanJson(1, [
    aPlannedNode({
      nodeId: "n2",
      title: "Login API",
      branch: "feature/auth/login-api",
      sessionId: "child-session-n2",
      prStatus: { phase: "open" },
      internalStatus: {
        kind: "needs-repoint",
        note: "parent n1 merged; base still points at feature/auth/token-store",
        source: "derived",
      },
    }),
  ]);

  // When
  openPrStackScreen(anOrchestratorSession(plan));

  // Then
  prStackScreenPage
    .internalStatusBadge("n2")
    .should("exist")
    .and("contain.text", "needs-repoint");
});

it("renders a ready-to-merge badge on a node whose dependencies are all merged", () => {
  // Given
  const plan = aStackPlanJson(1, [
    aPlannedNode({
      nodeId: "n1",
      title: "Token store",
      branch: "feature/auth/token-store",
      sessionId: "child-session-n1",
      prStatus: { phase: "open" },
      internalStatus: { kind: "ready-to-merge", source: "derived" },
    }),
  ]);

  // When
  openPrStackScreen(anOrchestratorSession(plan));

  // Then
  prStackScreenPage
    .internalStatusBadge("n1")
    .should("exist")
    .and("contain.text", "ready-to-merge");
});

it("shows no internal-status badge on a node that has no internal status yet", () => {
  // Given — a freshly planned node the developer has not acted on
  const plan = aStackPlanJson(1, [
    aPlannedNode({
      nodeId: "n1",
      title: "Token store",
      branch: "feature/auth/token-store",
      sessionId: "child-session-n1",
      prStatus: { phase: "open" },
    }),
  ]);

  // When
  openPrStackScreen(anOrchestratorSession(plan));

  // Then
  prStackScreenPage.statusChip("n1").should("exist");
  prStackScreenPage.internalStatusBadge("n1").should("not.exist");
});
