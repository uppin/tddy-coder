/**
 * Acceptance: `PrStackScreen`, mounted on its own through the package's entry point, lists the
 * planned PRs of the orchestrator's stack and lets the operator reorder them.
 *
 * The same behaviour is covered end to end through the sessions drawer (`PrStackReorderAcceptance`,
 * `PrStackPlannedPrListAcceptance`); these specs hold it to the package's own surface — one
 * in-memory backend, the clients handed in as props, no drawer in the tree.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Panel UX (C3, D24).
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { PrStackService } from "tddy-client-web/gen/pr_stack_pb";
import { SessionEntrySchema } from "tddy-client-web/gen/session_pb";
import { PrStackScreen } from "../../src/PrStackScreen";
import { mountWithPresenterRoom } from "../support/mountWorkflowChat";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { aPlannedNode, aStackPlanJson, type StackNodeFixture } from "../support/rpc/prStackFixtures";

const ORCHESTRATOR_SESSION_ID = "pr-stack-direct-mount-0000-0000-0000-000000000001";
const TITLES: Record<string, string> = { n1: "Add token store", n2: "Add auth middleware", n3: "Add login screen" };

/** Three independent roots, so nothing about the DAG can explain the order they render in. */
function anOrderedStack(order: string[]): StackNodeFixture[] {
  return ["n1", "n2", "n3"].map((nodeId) => aPlannedNode({ nodeId, title: TITLES[nodeId], displayOrder: order.indexOf(nodeId) }));
}

function anOrchestratorSession(order: string[]) {
  return create(SessionEntrySchema, {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-08-01T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    projectId: "proj-pr-stack",
    recipe: "pr-stack",
    stackPlanJson: aStackPlanJson(1, anOrderedStack(order)),
  });
}

/** Mount the screen over a daemon that answers a reorder with the plan in `reorderedTo` order. */
function mountScreen(order: string[], reorderedTo: string[]): InMemoryRpcBackend {
  const backend = anInMemoryRpcBackend().onUnary(PrStackService.method.reorderPlannedPr, () => ({
    stackPlanJson: aStackPlanJson(1, anOrderedStack(reorderedTo)),
  }));
  mountWithPresenterRoom(
    <PrStackScreen
      session={anOrchestratorSession(order)}
      prStackClient={createClient(PrStackService, backend.transport())}
      sessionToken="fake-token"
    />,
    backend,
  );
  return backend;
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("lists the stack's planned PRs in the order the plan persists", () => {
  // Given / When
  mountScreen(["n2", "n3", "n1"], ["n2", "n3", "n1"]);

  // Then
  prStackScreenPage.plannedPrRowNodeIds().should("deep.equal", ["n2", "n3", "n1"]);
});

it("moves a row one position earlier when its move-up control is clicked", () => {
  // Given
  mountScreen(["n1", "n2", "n3"], ["n2", "n1", "n3"]);
  prStackScreenPage.expandRow("n2");

  // When
  prStackScreenPage.clickMoveUp("n2");

  // Then — the list re-renders from the plan the daemon returned
  prStackScreenPage.plannedPrRowNodeIds().should("deep.equal", ["n2", "n1", "n3"]);
});

it("names the row and the direction it is being moved", () => {
  // Given
  const backend = mountScreen(["n1", "n2", "n3"], ["n1", "n3", "n2"]);
  prStackScreenPage.expandRow("n2");

  // When
  prStackScreenPage.clickMoveDown("n2");

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(PrStackService.method.reorderPlannedPr);
    expect(calls).to.have.length(1);
    expect(calls[0].sessionId).to.equal(ORCHESTRATOR_SESSION_ID);
    expect(calls[0].nodeId).to.equal("n2");
    expect(calls[0].direction).to.equal("down");
  });
});
