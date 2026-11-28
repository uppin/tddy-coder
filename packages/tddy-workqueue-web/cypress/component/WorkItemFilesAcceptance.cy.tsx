/**
 * Acceptance (work item files): an item that carries files shows an attachment count on its row and
 * lists them, name and size, in the expanded row; an item without files states nothing. Items carry
 * `files` in the queue JSON — no separate listing RPC.
 *
 * Feature: docs/ft/coder/work-item-files.md
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { WorkQueueView } from "../../src";
import { aPlannedNode, aWorkQueueJson } from "../support/rpc/prStackFixtures";

const THE_QUEUE = { projectId: "proj-item-files", queueId: "0190d000-0000-7000-8000-0000000000bb", daemonInstanceId: "" };

const ITEM_WITH_FILES = {
  ...aPlannedNode({ itemId: "n1", title: "Fix the token store", displayOrder: 0 }),
  files: [
    { basename: "failure.log", sizeBytes: 812 },
    { basename: "spec.md", sizeBytes: 2048 },
  ],
};
const ITEM_WITHOUT_FILES = {
  ...aPlannedNode({ itemId: "n2", title: "Draw the view", displayOrder: 1 }),
};

function mountTheQueue() {
  const backend = anInMemoryRpcBackend().onUnary(WorkQueueService.method.getWorkQueue, () =>
    create(WorkQueueService.method.getWorkQueue.output, {
      workQueueJson: aWorkQueueJson(1, [ITEM_WITH_FILES as never, ITEM_WITHOUT_FILES as never]),
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

it("an item with files shows an attachment count on its row; an item without shows none", () => {
  // When
  mountTheQueue();

  // Then
  byTestId("work-item-attachments-n1").should("be.visible").and("contain.text", "2");
  cy.get('[data-testid="work-item-attachments-n2"]').should("not.exist");
});

it("the expanded row lists the item's files with their names and sizes", () => {
  // Given
  mountTheQueue();

  // When
  byTestId("work-item-toggle-n1").click();

  // Then
  byTestId("work-item-files-n1").should("be.visible");
  byTestId("work-item-file-failure.log")
    .should("contain.text", "failure.log")
    .and("contain.text", "812 B");
  byTestId("work-item-file-spec.md")
    .should("contain.text", "spec.md")
    .and("contain.text", "2 KB");
});

it("the expanded row of an item without files lists no files", () => {
  // Given
  mountTheQueue();

  // When
  byTestId("work-item-toggle-n2").click();

  // Then
  cy.get('[data-testid="work-item-files-n2"]').should("not.exist");
});
