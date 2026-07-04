/**
 * Acceptance tests: manually creating a planned PR from the PR-Stack Chat Screen, and choosing
 * its ancestors, without going through the chat/agent refinement path.
 *
 * PRD: docs/ft/coder/pr-stacking.md § Manually adding a planned PR. Changeset:
 * docs/dev/1-WIP/pr-stack-manual-add-planned-pr.md.
 */

import React from "react";
import { Code } from "@connectrpc/connect";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { ConnectionService } from "../../src/gen/connection_pb";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { aPlannedNode, aStackPlanJson } from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-3333-0000-0000-0000-000000000030";

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
  const backend = aSessionsDrawerBackend([session]).onUnary(
    ConnectionService.method.addPlannedPr,
    () => ({
      stackPlanJson: aStackPlanJson(1, [
        aPlannedNode({ nodeId: "n1", title: "Add token store" }),
        aPlannedNode({ nodeId: "n2", title: "Add auth middleware", parents: ["n1"] }),
      ]),
    }),
  );
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(session.sessionId).click();
  return backend;
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

it("shows a New planned PR entry point on the planned-PR list", () => {
  // Given
  const plan = aStackPlanJson(1, [aPlannedNode({ nodeId: "n1", title: "Add token store" })]);

  // When
  openPrStackScreen(anOrchestratorSession(plan));

  // Then
  prStackScreenPage.addPlannedPrBtn().should("exist");
});

it("opens the New planned PR form when the entry point is clicked", () => {
  // Given
  const plan = aStackPlanJson(1, [aPlannedNode({ nodeId: "n1", title: "Add token store" })]);
  openPrStackScreen(anOrchestratorSession(plan));

  // When
  prStackScreenPage.openAddPlannedPrForm();

  // Then
  prStackScreenPage.addPlannedPrForm().should("exist");
  prStackScreenPage.addPlannedPrTitleInput().should("exist");
});

it("lists every existing planned PR as an ancestor checkbox option", () => {
  // Given — two existing nodes to choose as ancestors
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store" }),
    aPlannedNode({ nodeId: "n2", title: "Add auth middleware", parents: ["n1"] }),
  ]);
  openPrStackScreen(anOrchestratorSession(plan));

  // When
  prStackScreenPage.openAddPlannedPrForm();

  // Then
  prStackScreenPage.addPlannedPrAncestorCheckbox("n1").should("exist");
  prStackScreenPage.addPlannedPrAncestorCheckbox("n2").should("exist");
});

it("calls AddPlannedPr with the entered title and no parents when no ancestor is checked", () => {
  // Given
  const plan = aStackPlanJson(1, [aPlannedNode({ nodeId: "n1", title: "Add token store" })]);
  const backend = openPrStackScreen(anOrchestratorSession(plan));

  // When
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.fillAndSubmitAddPlannedPrForm({ title: "Add auth middleware" });

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.addPlannedPr);
    expect(calls).to.have.length(1);
    expect(calls[0].sessionId).to.equal(ORCHESTRATOR_SESSION_ID);
    expect(calls[0].title).to.equal("Add auth middleware");
    expect(calls[0].parents).to.deep.equal([]);
  });
});

it("calls AddPlannedPr with the checked ancestor node ids as parents", () => {
  // Given
  const plan = aStackPlanJson(1, [
    aPlannedNode({ nodeId: "n1", title: "Add token store" }),
    aPlannedNode({ nodeId: "n2", title: "Add auth middleware" }),
  ]);
  const backend = openPrStackScreen(anOrchestratorSession(plan));

  // When
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.fillAndSubmitAddPlannedPrForm({
    title: "Add token refresh endpoint",
    ancestorNodeIds: ["n1", "n2"],
  });

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.addPlannedPr);
    expect(calls).to.have.length(1);
    expect(calls[0].parents).to.deep.equal(["n1", "n2"]);
  });
});

it("passes the optional description and branch suggestion through to AddPlannedPr", () => {
  // Given
  const plan = aStackPlanJson(1, [aPlannedNode({ nodeId: "n1", title: "Add token store" })]);
  const backend = openPrStackScreen(anOrchestratorSession(plan));

  // When
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.fillAndSubmitAddPlannedPrForm({
    title: "Add auth middleware",
    description: "Validates the bearer token on every request.",
    branchSuggestion: "feature/auth-middleware",
    ancestorNodeIds: ["n1"],
  });

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.addPlannedPr);
    expect(calls[0].description).to.equal("Validates the bearer token on every request.");
    expect(calls[0].branchSuggestion).to.equal("feature/auth-middleware");
  });
});

it("renders the newly added planned PR in the list once AddPlannedPr succeeds", () => {
  // Given
  const plan = aStackPlanJson(1, [aPlannedNode({ nodeId: "n1", title: "Add token store" })]);
  openPrStackScreen(anOrchestratorSession(plan));

  // When — the stubbed backend responds with a stack that now includes "n2"
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.fillAndSubmitAddPlannedPrForm({
    title: "Add auth middleware",
    ancestorNodeIds: ["n1"],
  });

  // Then
  prStackScreenPage.plannedPrRow("n2").should("exist").and("contain.text", "Add auth middleware");
});

it("closes the form after AddPlannedPr succeeds", () => {
  // Given
  const plan = aStackPlanJson(1, [aPlannedNode({ nodeId: "n1", title: "Add token store" })]);
  openPrStackScreen(anOrchestratorSession(plan));

  // When
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.fillAndSubmitAddPlannedPrForm({ title: "Add auth middleware" });

  // Then
  prStackScreenPage.addPlannedPrForm().should("not.exist");
});

it("does not call AddPlannedPr when the title is left blank", () => {
  // Given
  const plan = aStackPlanJson(1, [aPlannedNode({ nodeId: "n1", title: "Add token store" })]);
  const backend = openPrStackScreen(anOrchestratorSession(plan));

  // When
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.addPlannedPrSubmitBtn().click();

  // Then
  cy.wrap(backend).should((b) => {
    expect(b.callsTo(ConnectionService.method.addPlannedPr)).to.have.length(0);
  });
  prStackScreenPage.addPlannedPrForm().should("exist");
});

it("shows an inline error and keeps the form open when AddPlannedPr fails", () => {
  // Given
  const plan = aStackPlanJson(1, [aPlannedNode({ nodeId: "n1", title: "Add token store" })]);
  const backend = aSessionsDrawerBackend([anOrchestratorSession(plan)]).failWith(
    ConnectionService.method.addPlannedPr,
    Code.InvalidArgument,
    "dangling parent ref",
  );
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();

  // When
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.fillAndSubmitAddPlannedPrForm({ title: "Add auth middleware" });

  // Then
  prStackScreenPage.addPlannedPrError().should("exist").and("contain.text", "dangling parent ref");
  prStackScreenPage.addPlannedPrForm().should("exist");
});

it("closes the form without adding a planned PR when Cancel is clicked", () => {
  // Given
  const plan = aStackPlanJson(1, [aPlannedNode({ nodeId: "n1", title: "Add token store" })]);
  const backend = openPrStackScreen(anOrchestratorSession(plan));

  // When
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.addPlannedPrCancelBtn().click();

  // Then
  prStackScreenPage.addPlannedPrForm().should("not.exist");
  cy.wrap(backend).should((b) => {
    expect(b.callsTo(ConnectionService.method.addPlannedPr)).to.have.length(0);
  });
});
