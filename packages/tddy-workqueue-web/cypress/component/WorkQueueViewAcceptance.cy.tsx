/**
 * Acceptance: the stack view (`WorkQueueView`, `#orchestrators` 9/11) shows one of a project's PR
 * stacks, opened by its address with no orchestrator session behind it — today's Planned PRs panel at
 * full width — and every control it offers acts on that stack by its address.
 *
 * Mounted on its own through the package's entry point, over one in-memory backend: the assertions
 * read the typed requests the view sent.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-10-stack-web-by-id.md
 * Changeset: docs/dev/1-WIP/2026-11-10-stack-web-by-id.md
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { WorkQueueView } from "../../src";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { workQueueViewPage, type ExpectedStackAddress } from "../support/pages/workQueueViewPage";
import { aPlannedNode, aWorkQueueJson, type WorkItemFixture } from "../support/rpc/prStackFixtures";

/** A stack held by another host than the one the view's clients reach. */
const THE_STACK: ExpectedStackAddress = {
  projectId: "proj-auth",
  queueId: "0190b000-0000-7000-8000-0000000000aa",
  daemonInstanceId: "host-b",
};

const TOKEN_STORE = aPlannedNode({ itemId: "n1", title: "Add token store", displayOrder: 0 });
const MIDDLEWARE = aPlannedNode({ itemId: "n2", title: "Add auth middleware", displayOrder: 1 });
const LOGOUT = aPlannedNode({ itemId: "n3", title: "Add logout", displayOrder: 2 });

function aPlan(nodes: WorkItemFixture[]): string {
  return aWorkQueueJson(1, nodes);
}

/**
 * The view over a daemon holding `THE_STACK` as n1, n2. A reorder answers with n2 first; an added
 * planned PR answers with n3 appended.
 */
function theStackView(): InMemoryRpcBackend {
  const backend = anInMemoryRpcBackend()
    .onUnary(WorkQueueService.method.getWorkQueue, () =>
      create(WorkQueueService.method.getWorkQueue.output, {
        workQueueJson: aPlan([TOKEN_STORE, MIDDLEWARE]),
      }),
    )
    .onUnary(WorkQueueService.method.reorderWorkItem, () =>
      create(WorkQueueService.method.reorderWorkItem.output, {
        workQueueJson: aPlan([
          { ...MIDDLEWARE, displayOrder: 0 },
          { ...TOKEN_STORE, displayOrder: 1 },
        ]),
      }),
    )
    .onUnary(WorkQueueService.method.addWorkItem, () =>
      create(WorkQueueService.method.addWorkItem.output, {
        workQueueJson: aPlan([TOKEN_STORE, MIDDLEWARE, LOGOUT]),
        itemId: "n3",
      }),
    );
  cy.mountWithClients(
    <WorkQueueView
      address={THE_STACK}
      workQueueClient={createClient(WorkQueueService, backend.transport())}
      sessionToken="fake-token"
    />,
    backend,
  );
  return backend;
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("the stack view lists the stack's planned prs and its controls act on that stack", () => {
  // Given
  const backend = theStackView();
  workQueueViewPage.view().should("exist");
  prStackScreenPage.workItemRowNodeIds().should("deep.equal", ["n1", "n2"]);
  workQueueViewPage.expectStackReadAt(backend, THE_STACK);

  // When — a row is moved, and a planned PR is added
  prStackScreenPage.expandRow("n2");
  prStackScreenPage.clickMoveUp("n2");
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.fillAndSubmitAddPlannedPrForm({ title: "Add logout" });

  // Then — both went to the stack by its address, and the list shows what the daemon answered
  workQueueViewPage.expectReorderSentTo(backend, THE_STACK, { itemId: "n2", direction: "up" });
  workQueueViewPage.expectPlannedPrAddedTo(backend, THE_STACK, "Add logout");
  prStackScreenPage.workItemRow("n3").should("contain.text", "Add logout");
});
