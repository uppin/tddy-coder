/**
 * Acceptance tests: the operator can reorder planned-PR rows, and the order persists.
 *
 * Row order used to be derived from the dependency graph, so a merge, a repoint or a re-parenting
 * silently rewrote the operator's view. Order is now a persisted per-node position that changes only
 * as a deliberate act — these controls being that act.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Panel UX (C3, D24; AC 11).
 */

import React from "react";
import { Code, ConnectError } from "@connectrpc/connect";
import { SessionsDrawerScreen } from "../../src/components/sessions/SessionsDrawerScreen";
import { ConnectionService, type SessionEntry } from "../../src/gen/connection_pb";
import { withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { mountWithRpc } from "../support/rpc/inMemory";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { aPlannedNode, aStackPlanJson, type StackNodeFixture } from "../support/rpc/prStackFixtures";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-7300-0000-0000-0000-000000000073";

/** Three independent roots, so nothing about the DAG can explain the order they render in. */
function anOrderedStack(order: string[]): StackNodeFixture[] {
  const titles: Record<string, string> = {
    n1: "Add token store",
    n2: "Add auth middleware",
    n3: "Add login screen",
  };
  return ["n1", "n2", "n3"].map((nodeId) =>
    aPlannedNode({ nodeId, title: titles[nodeId], displayOrder: order.indexOf(nodeId) }),
  );
}

function anOrchestratorSession(stackPlanJson: string): Partial<SessionEntry> {
  return {
    sessionId: ORCHESTRATOR_SESSION_ID,
    createdAt: "2026-08-01T09:00:00Z",
    status: "idle",
    repoPath: "/home/dev/pr-stack-project",
    isActive: false,
    projectId: "proj-pr-stack",
    recipe: "pr-stack",
    stackPlanJson,
  };
}

interface MountOptions {
  /** The order the rows start in. */
  order: string[];
  /** The order the daemon reports back after a successful reorder. */
  reorderedTo?: string[];
}

function aPrStackBackend(opts: MountOptions) {
  return aSessionsDrawerBackend([
    anOrchestratorSession(aStackPlanJson(1, anOrderedStack(opts.order))),
  ]).onUnary(ConnectionService.method.listTools, () => ({ tools: [] }));
}

function mountAndOpen(backend: ReturnType<typeof aPrStackBackend>) {
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  return backend;
}

/** Open the screen with a `ReorderPlannedPr` that succeeds and returns the reordered plan. */
function openPrStackScreen(opts: MountOptions) {
  return mountAndOpen(
    aPrStackBackend(opts).onUnary(ConnectionService.method.reorderPlannedPr, () => ({
      stackPlanJson: aStackPlanJson(1, anOrderedStack(opts.reorderedTo ?? opts.order)),
    })),
  );
}

/** Open the screen with a `ReorderPlannedPr` the daemon refuses, carrying `message` as its reason. */
function openPrStackScreenWithRefusedReorder(message: string, opts: MountOptions) {
  return mountAndOpen(
    aPrStackBackend(opts).onUnary(ConnectionService.method.reorderPlannedPr, () => {
      throw new ConnectError(message, Code.InvalidArgument);
    }),
  );
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
// Moving a row
// ---------------------------------------------------------------------------

it("moves a row one position earlier when its move-up control is clicked", () => {
  // Given
  openPrStackScreen({ order: ["n1", "n2", "n3"], reorderedTo: ["n2", "n1", "n3"] });
  prStackScreenPage.expandRow("n2");

  // When
  prStackScreenPage.clickMoveUp("n2");

  // Then — the list re-renders from the plan the daemon returned
  prStackScreenPage.plannedPrRowNodeIds().should("deep.equal", ["n2", "n1", "n3"]);
});

it("moves a row one position later when its move-down control is clicked", () => {
  // Given
  openPrStackScreen({ order: ["n1", "n2", "n3"], reorderedTo: ["n2", "n1", "n3"] });
  prStackScreenPage.expandRow("n1");

  // When
  prStackScreenPage.clickMoveDown("n1");

  // Then
  prStackScreenPage.plannedPrRowNodeIds().should("deep.equal", ["n2", "n1", "n3"]);
});

it("names the row and the direction it is being moved", () => {
  // Given
  const backend = openPrStackScreen({ order: ["n1", "n2", "n3"], reorderedTo: ["n1", "n3", "n2"] });
  prStackScreenPage.expandRow("n2");

  // When
  prStackScreenPage.clickMoveDown("n2");

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(ConnectionService.method.reorderPlannedPr);
    expect(calls).to.have.length(1);
    expect(calls[0].sessionId).to.equal(ORCHESTRATOR_SESSION_ID);
    expect(calls[0].nodeId).to.equal("n2");
    expect(calls[0].direction).to.equal("down");
  });
});

// ---------------------------------------------------------------------------
// The ends of the list
// ---------------------------------------------------------------------------

it("offers no move-up on the first row", () => {
  // Given / When
  openPrStackScreen({ order: ["n1", "n2", "n3"] });
  prStackScreenPage.expandRow("n1");

  // Then — an inert control that looks live is worse than no control
  prStackScreenPage.moveUpBtn("n1").should("be.disabled");
  prStackScreenPage.moveDownBtn("n1").should("be.enabled");
});

it("offers no move-down on the last row", () => {
  // Given / When
  openPrStackScreen({ order: ["n1", "n2", "n3"] });
  prStackScreenPage.expandRow("n3");

  // Then
  prStackScreenPage.moveDownBtn("n3").should("be.disabled");
  prStackScreenPage.moveUpBtn("n3").should("be.enabled");
});

// ---------------------------------------------------------------------------
// A refused reorder
// ---------------------------------------------------------------------------

it("leaves the rows where they were when the daemon refuses the reorder", () => {
  // Given
  openPrStackScreenWithRefusedReorder("node 'n2' is not in this stack", {
    order: ["n1", "n2", "n3"],
  });
  prStackScreenPage.expandRow("n2");

  // When
  prStackScreenPage.clickMoveUp("n2");

  // Then — nothing was persisted, so nothing may appear to have moved
  prStackScreenPage.plannedPrRowNodeIds().should("deep.equal", ["n1", "n2", "n3"]);
});

it("states the daemon's reason when it refuses the reorder", () => {
  // Given
  openPrStackScreenWithRefusedReorder("node 'n2' is not in this stack", {
    order: ["n1", "n2", "n3"],
  });
  prStackScreenPage.expandRow("n2");

  // When
  prStackScreenPage.clickMoveUp("n2");

  // Then — a row that did not move is indistinguishable from a click that was swallowed
  prStackScreenPage.reorderError("n2").should("have.text", "[invalid_argument] node 'n2' is not in this stack");
});

it("keeps a reorder failure visible after the row is collapsed", () => {
  // Given — the move controls sit inside the collapse boundary; the reason must not
  openPrStackScreenWithRefusedReorder("node 'n2' is not in this stack", {
    order: ["n1", "n2", "n3"],
  });
  prStackScreenPage.expandRow("n2");
  prStackScreenPage.clickMoveUp("n2");
  prStackScreenPage.reorderError("n2").should("be.visible");

  // When
  prStackScreenPage.expandRow("n2");

  // Then — a reason the operator must expand a row to find is a fresh dead end
  prStackScreenPage.rowDetails("n2").should("not.be.visible");
  prStackScreenPage.reorderError("n2").should("be.visible");
});
