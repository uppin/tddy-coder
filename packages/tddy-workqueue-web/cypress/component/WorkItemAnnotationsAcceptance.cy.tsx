/**
 * Acceptance (`#workqueue` 7/14): labels show as chips and badges by tone, apart from the derived
 * internal-status badge, and the queue view filters by a chosen label. Items carry `labels` and
 * `badges` in the queue JSON.
 *
 * Feature: docs/ft/coder/pr-stacking.md § Labels and badges on a work item
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { WorkQueueView } from "../../src";
import { aPlannedNode, aWorkQueueJson } from "../support/rpc/prStackFixtures";

const THE_QUEUE = { projectId: "proj-annotations", queueId: "0190d000-0000-7000-8000-0000000000aa", daemonInstanceId: "" };

const API_ITEM = {
  ...aPlannedNode({ itemId: "n1", title: "Wire the API", displayOrder: 0 }),
  labels: ["api", "needs-review"],
  badges: [{ key: "qa", text: "passed", tone: "success" }],
  internalStatus: { kind: "needs-repoint", source: "derived" },
};
const UI_ITEM = {
  ...aPlannedNode({ itemId: "n2", title: "Draw the view", displayOrder: 1 }),
  labels: ["ui"],
  badges: [],
};

function mountTheQueue() {
  const backend = anInMemoryRpcBackend().onUnary(WorkQueueService.method.getWorkQueue, () =>
    create(WorkQueueService.method.getWorkQueue.output, {
      workQueueJson: aWorkQueueJson(1, [API_ITEM as never, UI_ITEM as never]),
    }),
  );
  cy.mountWithClients(
    <WorkQueueView
      address={THE_QUEUE}
      workQueueClient={createClient(WorkQueueService, backend.transport())}
      watchClient={createClient(WorkQueueService, backend.transport())}
      sessionToken="fake-token"
    />,
    backend,
  );
}

const byTestId = (id: string) => cy.get(`[data-testid="${id}"]`, { timeout: 5000 });

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("labels show as chips and badges by tone, apart from the internal status badge", () => {
  // When
  mountTheQueue();

  // Then
  byTestId("work-item-label-n1-api").should("be.visible");
  byTestId("work-item-label-n1-needs-review").should("be.visible");
  byTestId("work-item-badge-n1-qa").should("have.attr", "data-tone", "success").and("contain.text", "passed");
  byTestId("work-item-badge-n1-qa").should("not.contain.text", "needs-repoint");
});

it("choosing a label filters the queue to items carrying it", () => {
  // Given
  mountTheQueue();

  // When
  byTestId("work-queue-label-filter").select("ui");

  // Then
  byTestId("work-item-label-n2-ui").should("be.visible");
  cy.get('[data-testid="work-item-label-n1-api"]').should("not.exist");
});
