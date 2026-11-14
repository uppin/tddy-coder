/**
 * Acceptance: a project's PR stacks in the sessions drawer (`#orchestrators` 9/11). Under each project
 * header a **Stacks** row lists the project's stacks on every host (`ListProjectStacks`); opening one
 * shows the stack view at `#/sessions/project/<projectId>/stack/<stackId>`, read by the stack's
 * address from the host that holds it. A pr-stack orchestrator's chat screen keeps its Planned PRs
 * panel, now acting on its stack by the stack's id.
 *
 * The whole Sessions screen over the in-memory ConnectRPC backend (`mountWithRpc`), so the
 * assertions read the typed requests the screen sent.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-stack-web-by-id.md
 * Changeset: docs/dev/1-WIP/2026-11-10-stack-web-by-id.md
 */

import React from "react";
import { create } from "@bufbuild/protobuf";
import type { ProjectEntry } from "tddy-client-web/gen/project_pb";
import { PrStackService, ProjectStackSummarySchema } from "tddy-client-web/gen/pr_stack_pb";
import { SessionsDrawerScreen } from "../../src/SessionsDrawerScreen";
import { DEFAULT_TEST_DAEMON, withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aSessionServiceBackend } from "../support/rpc/daemonSessionHostBackend";
import { aSessionsDrawerBackend } from "../support/rpc/vncBackend";
import { mountWithRpc } from "../support/rpc/inMemory";
import { sessionsDrawerPage } from "../support/pages/sessionsDrawerPage";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { projectStacksPage } from "../support/pages/projectStacksPage";
import { aPlannedNode, aStackPlanJson } from "../support/rpc/prStackFixtures";

const BETA = "proj-beta";
/** A stack of beta held by the selected daemon, and one held by another host. */
const AUTH_STACK = { projectId: BETA, stackId: "0190b000-0000-7000-8000-0000000000aa", daemonInstanceId: DEFAULT_TEST_DAEMON.instanceId };
const BILLING_STACK = { projectId: BETA, stackId: "0190b000-0000-7000-8000-0000000000bb", daemonInstanceId: "host-b" };

const BILLING_PLAN = aStackPlanJson(1, [
  aPlannedNode({ nodeId: "n1", title: "Add invoices table", displayOrder: 0 }),
  aPlannedNode({ nodeId: "n2", title: "Add billing page", displayOrder: 1 }),
]);

/** A project the selected daemon lists. */
function aListedProject(projectId: string, name: string): Partial<ProjectEntry> {
  return { projectId, name, mainBranchRef: "origin/master", daemonInstanceId: DEFAULT_TEST_DAEMON.instanceId };
}

function aStackSummary(address: typeof AUTH_STACK, title: string, total: number) {
  return create(ProjectStackSummarySchema, { address, title, nodeCounts: { total } });
}

/** The Sessions screen over a daemon listing beta, whose stacks are `AUTH_STACK` and `BILLING_STACK`. */
function theSessionsScreenWithBetasStacks() {
  const backend = aSessionServiceBackend({
    sessions: [{ sessionId: "b1", projectId: BETA }],
    projectsOverride: [aListedProject(BETA, "beta")],
  })
    .onUnary(PrStackService.method.listProjectStacks, (req) =>
      create(PrStackService.method.listProjectStacks.output, {
        stacks:
          req.projectId === BETA
            ? [aStackSummary(AUTH_STACK, "Auth rework", 3), aStackSummary(BILLING_STACK, "Billing", 2)]
            : [],
      }),
    )
    .onUnary(PrStackService.method.getProjectStack, () =>
      create(PrStackService.method.getProjectStack.output, {
        summary: aStackSummary(BILLING_STACK, "Billing", 2),
        stackPlanJson: BILLING_PLAN,
      }),
    );
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  return backend;
}

const ORCHESTRATOR_SESSION_ID = "pr-stack-session-9999-0000-0000-0000-000000000090";
const ORCHESTRATORS_STACK = { projectId: "proj-pr-stack", stackId: "0190b000-0000-7000-8000-0000000000cc" };
const ORCHESTRATORS_PLAN = aStackPlanJson(1, [aPlannedNode({ nodeId: "n1", title: "Add token store" })]);

/** The Sessions screen with a pr-stack orchestrator whose changeset names `ORCHESTRATORS_STACK`. */
function theSessionsScreenWithAPrStackOrchestrator() {
  const backend = aSessionsDrawerBackend([
    {
      sessionId: ORCHESTRATOR_SESSION_ID,
      createdAt: "2026-11-14T09:00:00Z",
      status: "idle",
      repoPath: "/home/dev/pr-stack-project",
      pid: 0,
      isActive: false,
      projectId: ORCHESTRATORS_STACK.projectId,
      daemonInstanceId: "",
      workflowGoal: "",
      pendingElicitation: false,
      orchestratorSessionId: "",
      recipe: "pr-stack",
      stackPlanJson: ORCHESTRATORS_PLAN,
      stackId: ORCHESTRATORS_STACK.stackId,
    },
  ])
    .onUnary(PrStackService.method.getProjectStack, () =>
      create(PrStackService.method.getProjectStack.output, { stackPlanJson: ORCHESTRATORS_PLAN }),
    )
    .onUnary(PrStackService.method.addPlannedPr, () =>
      create(PrStackService.method.addPlannedPr.output, {
        stackPlanJson: aStackPlanJson(1, [
          aPlannedNode({ nodeId: "n1", title: "Add token store" }),
          aPlannedNode({ nodeId: "n2", title: "Add auth middleware" }),
        ]),
        nodeId: "n2",
      }),
    );
  mountWithRpc(withSelectedDaemon(<SessionsDrawerScreen />), backend);
  return backend;
}

beforeEach(() => {
  cy.viewport(1280, 800);
  cy.clearLocalStorage();
  cy.clearAllSessionStorage();
  window.localStorage.setItem("tddy_session_token", "fake-token");
  window.location.hash = "#/sessions";
});

it("a project's stacks are listed under its header and open in the stack view", () => {
  // Given
  const backend = theSessionsScreenWithBetasStacks();
  projectStacksPage.assertStacksListedFor(backend, BETA);
  projectStacksPage.stacksRow(BETA).should("contain", "Auth rework").and("contain", "Billing");

  // When
  projectStacksPage.openStack(BILLING_STACK.stackId);

  // Then — the stack view, read from the host that holds the stack
  projectStacksPage.assertAtStackView(BETA, BILLING_STACK.stackId);
  projectStacksPage.stackView().should("be.visible");
  prStackScreenPage.plannedPrRowNodeIds().should("deep.equal", ["n1", "n2"]);
  projectStacksPage.assertStackReadAt(backend, BILLING_STACK);
});

it("a pr-stack orchestrator's chat screen shows its stack as before", () => {
  // Given
  const backend = theSessionsScreenWithAPrStackOrchestrator();
  sessionsDrawerPage.drawerItem(ORCHESTRATOR_SESSION_ID).click();
  prStackScreenPage.plannedPrRow("n1").should("contain.text", "Add token store");

  // When
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.fillAndSubmitAddPlannedPrForm({ title: "Add auth middleware" });

  // Then — the panel acts on the orchestrator's stack by its id, and shows the daemon's answer
  projectStacksPage.assertPlannedPrAddedTo(backend, ORCHESTRATORS_STACK, "Add auth middleware");
  prStackScreenPage.plannedPrRow("n2").should("contain.text", "Add auth middleware");
});
