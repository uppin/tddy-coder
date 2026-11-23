/**
 * Acceptance: a project's PR stacks in the sessions drawer (`#orchestrators` 9/11). Under each project
 * header a **Stacks** row lists the project's work queues on every host (`ListProjectWorkQueues`);
 * opening one opens its owning session's inspector on the Work queue tab (`#workqueue` 5/14 — a queue's
 * id is its session's), read by the queue's address from the host that holds it.
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
import { WorkQueueService, WorkQueueSummarySchema } from "tddy-client-web/gen/work_queue_pb";
import { SessionsDrawerScreen } from "../../src/SessionsDrawerScreen";
import { DEFAULT_TEST_DAEMON, withSelectedDaemon } from "../support/rpc/withSelectedDaemon";
import { aSessionServiceBackend } from "../support/rpc/daemonSessionHostBackend";
import { mountWithRpc } from "../support/rpc/inMemory";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { projectStacksPage } from "../support/pages/projectStacksPage";
import { aPlannedNode, aWorkQueueJson } from "../support/rpc/prStackFixtures";

const BETA = "proj-beta";
/** A stack of beta held by the selected daemon, and one held by another host. */
const AUTH_STACK = { projectId: BETA, queueId: "0190b000-0000-7000-8000-0000000000aa", daemonInstanceId: DEFAULT_TEST_DAEMON.instanceId };
const BILLING_STACK = { projectId: BETA, queueId: "0190b000-0000-7000-8000-0000000000bb", daemonInstanceId: "host-b" };

const BILLING_PLAN = aWorkQueueJson(1, [
  aPlannedNode({ itemId: "n1", title: "Add invoices table", displayOrder: 0 }),
  aPlannedNode({ itemId: "n2", title: "Add billing page", displayOrder: 1 }),
]);

/** A project the selected daemon lists. */
function aListedProject(projectId: string, name: string): Partial<ProjectEntry> {
  return { projectId, name, mainBranchRef: "origin/master", daemonInstanceId: DEFAULT_TEST_DAEMON.instanceId };
}

function aStackSummary(address: typeof AUTH_STACK, title: string, total: number) {
  return create(WorkQueueSummarySchema, { address, title, nodeCounts: { total } });
}

/** The Sessions screen over a daemon listing beta, whose stacks are `AUTH_STACK` and `BILLING_STACK`. */
function theSessionsScreenWithBetasStacks() {
  const backend = aSessionServiceBackend({
    sessions: [
      { sessionId: "b1", projectId: BETA },
      // The sessions that own the two queues
      { sessionId: AUTH_STACK.queueId, projectId: BETA, ownsWorkQueue: true },
      {
        sessionId: BILLING_STACK.queueId,
        projectId: BETA,
        ownsWorkQueue: true,
        daemonInstanceId: BILLING_STACK.daemonInstanceId,
      },
    ],
    projectsOverride: [aListedProject(BETA, "beta")],
  })
    .onUnary(WorkQueueService.method.listProjectWorkQueues, (req) =>
      create(WorkQueueService.method.listProjectWorkQueues.output, {
        stacks:
          req.projectId === BETA
            ? [aStackSummary(AUTH_STACK, "Auth rework", 3), aStackSummary(BILLING_STACK, "Billing", 2)]
            : [],
      }),
    )
    .onUnary(WorkQueueService.method.getWorkQueue, () =>
      create(WorkQueueService.method.getWorkQueue.output, {
        summary: aStackSummary(BILLING_STACK, "Billing", 2),
        workQueueJson: BILLING_PLAN,
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
  projectStacksPage.openStack(BILLING_STACK.queueId);

  // Then — the stack view, read from the host that holds the stack
  projectStacksPage.assertAtStackView(BETA, BILLING_STACK.queueId);
  projectStacksPage.stackView().should("be.visible");
  prStackScreenPage.workItemRowNodeIds().should("deep.equal", ["n1", "n2"]);
  projectStacksPage.assertStackReadAt(backend, BILLING_STACK);
});
