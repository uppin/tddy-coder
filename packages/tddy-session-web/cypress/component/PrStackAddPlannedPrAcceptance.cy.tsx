/**
 * Acceptance tests: manually creating a planned PR from the PR-Stack Chat Screen, and choosing
 * its ancestors, without going through the chat/agent refinement path.
 *
 * PRD: docs/ft/coder/pr-stacking.md § Manually adding a planned PR. Changeset:
 * docs/dev/1-WIP/pr-stack-manual-add-planned-pr.md.
 */

import React from "react";
import { Code } from "@connectrpc/connect";
import { SessionService } from "tddy-client-web/gen/session_pb";
import { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { openProjectStackViewOf } from "../support/rpc/openProjectStackView";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { aPlannedNode, aWorkQueueJson } from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-3333-0000-0000-0000-000000000030";

function anOrchestratorSession(workQueueJson: string) {
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
    workQueueJson,
  };
}

function openPrStackScreen(session: ReturnType<typeof anOrchestratorSession>) {
  const backend = aSessionsDrawerBackend([session]).onUnary(
    WorkQueueService.method.addWorkItem,
    () => ({
      workQueueJson: aWorkQueueJson(1, [
        aPlannedNode({ itemId: "n1", title: "Add token store" }),
        aPlannedNode({ itemId: "n2", title: "Add auth middleware", parents: ["n1"] }),
      ]),
    }),
  );
  openProjectStackViewOf(backend, session.sessionId);
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
  const plan = aWorkQueueJson(1, [aPlannedNode({ itemId: "n1", title: "Add token store" })]);

  // When
  openPrStackScreen(anOrchestratorSession(plan));

  // Then
  prStackScreenPage.addWorkItemBtn().should("exist");
});

it("opens the New planned PR form when the entry point is clicked", () => {
  // Given
  const plan = aWorkQueueJson(1, [aPlannedNode({ itemId: "n1", title: "Add token store" })]);
  openPrStackScreen(anOrchestratorSession(plan));

  // When
  prStackScreenPage.openAddPlannedPrForm();

  // Then
  prStackScreenPage.addWorkItemForm().should("exist");
  prStackScreenPage.addWorkItemTitleInput().should("exist");
});

it("lists every existing planned PR as an ancestor checkbox option", () => {
  // Given — two existing nodes to choose as ancestors
  const plan = aWorkQueueJson(1, [
    aPlannedNode({ itemId: "n1", title: "Add token store" }),
    aPlannedNode({ itemId: "n2", title: "Add auth middleware", parents: ["n1"] }),
  ]);
  openPrStackScreen(anOrchestratorSession(plan));

  // When
  prStackScreenPage.openAddPlannedPrForm();

  // Then
  prStackScreenPage.addWorkItemAncestorCheckbox("n1").should("exist");
  prStackScreenPage.addWorkItemAncestorCheckbox("n2").should("exist");
});

it("calls AddWorkItem with the entered title and no parents when no ancestor is checked", () => {
  // Given
  const plan = aWorkQueueJson(1, [aPlannedNode({ itemId: "n1", title: "Add token store" })]);
  const backend = openPrStackScreen(anOrchestratorSession(plan));

  // When
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.fillAndSubmitAddPlannedPrForm({ title: "Add auth middleware" });

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(WorkQueueService.method.addWorkItem);
    expect(calls).to.have.length(1);
    expect(calls[0].stack?.queueId).to.equal(ORCHESTRATOR_SESSION_ID);
    expect(calls[0].title).to.equal("Add auth middleware");
    expect(calls[0].parents).to.deep.equal([]);
  });
});

it("calls AddWorkItem with the checked ancestor node ids as parents", () => {
  // Given
  const plan = aWorkQueueJson(1, [
    aPlannedNode({ itemId: "n1", title: "Add token store" }),
    aPlannedNode({ itemId: "n2", title: "Add auth middleware" }),
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
    const calls = b.callsTo(WorkQueueService.method.addWorkItem);
    expect(calls).to.have.length(1);
    expect(calls[0].parents).to.deep.equal(["n1", "n2"]);
  });
});

it("passes the optional description and branch suggestion through to AddWorkItem", () => {
  // Given
  const plan = aWorkQueueJson(1, [aPlannedNode({ itemId: "n1", title: "Add token store" })]);
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
    const calls = b.callsTo(WorkQueueService.method.addWorkItem);
    expect(calls[0].description).to.equal("Validates the bearer token on every request.");
    expect(calls[0].branchSuggestion).to.equal("feature/auth-middleware");
  });
});

it("renders the newly added planned PR in the list once AddWorkItem succeeds", () => {
  // Given
  const plan = aWorkQueueJson(1, [aPlannedNode({ itemId: "n1", title: "Add token store" })]);
  openPrStackScreen(anOrchestratorSession(plan));

  // When — the stubbed backend responds with a stack that now includes "n2"
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.fillAndSubmitAddPlannedPrForm({
    title: "Add auth middleware",
    ancestorNodeIds: ["n1"],
  });

  // Then
  prStackScreenPage.workItemRow("n2").should("exist").and("contain.text", "Add auth middleware");
});

it("closes the form after AddWorkItem succeeds", () => {
  // Given
  const plan = aWorkQueueJson(1, [aPlannedNode({ itemId: "n1", title: "Add token store" })]);
  openPrStackScreen(anOrchestratorSession(plan));

  // When
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.fillAndSubmitAddPlannedPrForm({ title: "Add auth middleware" });

  // Then
  prStackScreenPage.addWorkItemForm().should("not.exist");
});

it("does not call AddWorkItem when the title is left blank", () => {
  // Given
  const plan = aWorkQueueJson(1, [aPlannedNode({ itemId: "n1", title: "Add token store" })]);
  const backend = openPrStackScreen(anOrchestratorSession(plan));

  // When
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.addWorkItemSubmitBtn().click();

  // Then
  cy.wrap(backend).should((b) => {
    expect(b.callsTo(WorkQueueService.method.addWorkItem)).to.have.length(0);
  });
  prStackScreenPage.addWorkItemForm().should("exist");
});

it("shows an inline error and keeps the form open when AddWorkItem fails", () => {
  // Given
  const plan = aWorkQueueJson(1, [aPlannedNode({ itemId: "n1", title: "Add token store" })]);
  const backend = aSessionsDrawerBackend([anOrchestratorSession(plan)]).failWith(
    WorkQueueService.method.addWorkItem,
    Code.InvalidArgument,
    "dangling parent ref",
  );
  openProjectStackViewOf(backend, ORCHESTRATOR_SESSION_ID);

  // When
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.fillAndSubmitAddPlannedPrForm({ title: "Add auth middleware" });

  // Then
  prStackScreenPage.addWorkItemError().should("exist").and("contain.text", "dangling parent ref");
  prStackScreenPage.addWorkItemForm().should("exist");
});

it("closes the form without adding a planned PR when Cancel is clicked", () => {
  // Given
  const plan = aWorkQueueJson(1, [aPlannedNode({ itemId: "n1", title: "Add token store" })]);
  const backend = openPrStackScreen(anOrchestratorSession(plan));

  // When
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.addWorkItemCancelBtn().click();

  // Then
  prStackScreenPage.addWorkItemForm().should("not.exist");
  cy.wrap(backend).should((b) => {
    expect(b.callsTo(WorkQueueService.method.addWorkItem)).to.have.length(0);
  });
});
