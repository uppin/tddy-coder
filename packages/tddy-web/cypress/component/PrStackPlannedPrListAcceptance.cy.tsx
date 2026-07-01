/**
 * Acceptance tests: the PR-Stack Chat Screen's planned-PR list.
 *
 * Renders one row per `StackNode` in the orchestrator session's `Stack` (topo order),
 * showing a "Start session" CTA for unspawned nodes and a status chip for spawned ones.
 *
 * PRD: docs/ft/coder/pr-stacking.md (unified pr-stack recipe), docs/ft/web/session-drawer.md
 * § PR-Stack Chat Screen. Changeset: docs/dev/1-WIP/pr-stack-workflow-views.md.
 */

import React from "react";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { aPlannedNode, aStackPlanJson } from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-1111-0000-0000-0000-000000000010";

function anOrchestratorSession(stackPlanJson: string) {
  return {
    sessionId: ORCHESTRATOR_SESSION_ID,
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
    recipe: "pr-stack",
    stackPlanJson,
  };
}

function openPrStackScreen(session: ReturnType<typeof anOrchestratorSession>) {
  const backend = aSessionsDrawerBackend([session]);
  mountWithRpc(<SessionsDrawerScreen />, backend);
  sessionsDrawerPage.drawerItem(session.sessionId).click();
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

it("shows a planned-PR row with a Start session CTA for a node that has not been spawned", () => {
  // Given
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store" }),
  ]);

  // When
  openPrStackScreen(anOrchestratorSession(plan));

  // Then
  prStackScreenPage.plannedPrRow("n1").should("exist").and("contain.text", "Add token store");
  prStackScreenPage.startSessionBtn("n1").should("exist");
  prStackScreenPage.statusChip("n1").should("not.exist");
});

it("shows a status chip instead of a Start session CTA once a node has a spawned child session", () => {
  // Given
  const plan = aStackPlanJson(1, [
    aPlannedNode({
      nodeId: "n1",
      title: "Add token store",
      branch: "feature/token-store",
      sessionId: "child-session-abc",
      prStatus: { phase: "open" },
    }),
  ]);

  // When
  openPrStackScreen(anOrchestratorSession(plan));

  // Then
  prStackScreenPage.statusChip("n1").should("exist").and("contain.text", "open");
  prStackScreenPage.startSessionBtn("n1").should("not.exist");
});

it("renders planned-PR rows in topological order, roots before their dependents", () => {
  // Given — n2 depends on n1; the plan lists them out of order to prove sorting, not fixture order
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n2", title: "Add auth middleware", parents: ["n1"] }),
    aPlannedNode({ nodeId: "n1", title: "Add token store" }),
  ]);

  // When
  openPrStackScreen(anOrchestratorSession(plan));

  // Then
  prStackScreenPage.plannedPrRowNodeIds().should("deep.equal", ["n1", "n2"]);
});
