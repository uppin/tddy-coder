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
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
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
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
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
  // Given — n2 depends on n1; the plan lists them out of order to prove sorting, not fixture order.
  // Neither node carries a persisted position, which is what a plan authored before display order
  // existed looks like — so this is also the legacy fallback case.
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n2", title: "Add auth middleware", parents: ["n1"] }),
    aPlannedNode({ nodeId: "n1", title: "Add token store" }),
  ]);

  // When
  openPrStackScreen(anOrchestratorSession(plan));

  // Then
  prStackScreenPage.plannedPrRowNodeIds().should("deep.equal", ["n1", "n2"]);
});

// ---------------------------------------------------------------------------
// Row order is read from the plan, not re-derived from the DAG
// ---------------------------------------------------------------------------

it("renders planned-PR rows in the order the plan persists", () => {
  // Given — the persisted positions contradict both the fixture order and the dependency graph
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store", displayOrder: 2 }),
    aPlannedNode({ nodeId: "n2", title: "Add auth middleware", parents: ["n1"], displayOrder: 0 }),
    aPlannedNode({ nodeId: "n3", title: "Add login screen", parents: ["n2"], displayOrder: 1 }),
  ]);

  // When
  openPrStackScreen(anOrchestratorSession(plan));

  // Then — the operator's reading order and the dependency graph are allowed to differ
  prStackScreenPage.plannedPrRowNodeIds().should("deep.equal", ["n2", "n3", "n1"]);
});

it("keeps a row in its persisted position when a predecessor merges under it", () => {
  // Given — n1 has merged, which collapses n2's effective base and would re-layer a derived order.
  // The positions say otherwise, and a merge is exactly the kind of unrelated event that used to
  // make a row the operator was reading jump.
  const plan = aStackPlanJson(1, [
    aPlannedNode({
      nodeId: "n1",
      title: "Add token store",
      branch: "feature/auth/token-store",
      prStatus: { phase: "merged" },
      displayOrder: 1,
    }),
    aPlannedNode({
      nodeId: "n2",
      title: "Add auth middleware",
      parents: ["n1"],
      displayOrder: 0,
    }),
  ]);

  // When
  openPrStackScreen(anOrchestratorSession(plan));

  // Then
  prStackScreenPage.plannedPrRowNodeIds().should("deep.equal", ["n2", "n1"]);
});

it("falls back to topological order when only some rows carry a persisted position", () => {
  // Given — a half-numbered plan has no coherent total order. Interleaving real positions with
  // invented ones can render a child above its parent, which is a worse lie than one render of a
  // correct derived order; the next write to the stack numbers everything.
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n2", title: "Add auth middleware", parents: ["n1"], displayOrder: 0 }),
    aPlannedNode({ nodeId: "n1", title: "Add token store" }),
  ]);

  // When
  openPrStackScreen(anOrchestratorSession(plan));

  // Then
  prStackScreenPage.plannedPrRowNodeIds().should("deep.equal", ["n1", "n2"]);
});
