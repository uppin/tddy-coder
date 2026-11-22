/**
 * Acceptance (`#workqueue` 6/14): the user assigns an existing PR to a work item and links a tddy session
 * to it from its row; a PR'ed item shows its running, failed and succeeded check counts.
 *
 * Test ids are final (`#workqueue` 3/14's `work-item-…`); the view and its RPCs are mounted under
 * today's names, which `/green` follows through n3's rename.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-11-22-pr-and-session-link.md
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { WorkQueueView, WorkItemChecksSummary } from "../../src";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { aPlannedNode, aWorkQueueJson } from "../support/rpc/prStackFixtures";

const THE_QUEUE = { projectId: "proj-links", queueId: "0190c000-0000-7000-8000-0000000000aa", daemonInstanceId: "" };
const AN_ITEM = aPlannedNode({ itemId: "n1", title: "Wire the inbox", displayOrder: 0 });
const A_SESSION = {
  sessionId: "sess-reviewer-0000-0000-0000-000000000001",
  projectId: "proj-links",
  repoPath: "/home/dev/reviewer",
  status: "idle",
  isActive: false,
};

function theQueueView(): InMemoryRpcBackend {
  const plan = aWorkQueueJson(1, [AN_ITEM]);
  const backend = anInMemoryRpcBackend()
    .onUnary(WorkQueueService.method.getWorkQueue, () =>
      create(WorkQueueService.method.getWorkQueue.output, { workQueueJson: plan }),
    )
    .onUnary(WorkQueueService.method.assignWorkItemPr, () =>
      create(WorkQueueService.method.assignWorkItemPr.output, { workQueueJson: plan }),
    )
    .onUnary(WorkQueueService.method.linkWorkItemSession, () =>
      create(WorkQueueService.method.linkWorkItemSession.output, { workQueueJson: plan }),
    );
  cy.mountWithClients(
    <WorkQueueView
      address={THE_QUEUE}
      workQueueClient={createClient(WorkQueueService, backend.transport())}
      sessionToken="fake-token"
      sessions={[A_SESSION] as never}
    />,
    backend,
  );
  return backend;
}

const byTestId = (id: string) => cy.get(`[data-testid="${id}"]`, { timeout: 5000 });

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("the user assigns a PR to an item from its row", () => {
  // Given
  const backend = theQueueView();
  prStackScreenPage.expandRow("n1");

  // When
  byTestId("work-item-assign-pr-n1").click();
  byTestId("work-item-assign-pr-input-n1").type("7");
  byTestId("work-item-assign-pr-submit-n1").click();

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(WorkQueueService.method.assignWorkItemPr);
    expect(calls).to.have.length(1);
    expect(calls[0].itemId).to.equal("n1");
    expect(calls[0].pullNumber).to.equal(7n);
  });
});

it("the user links a session to an item from its row", () => {
  // Given
  const backend = theQueueView();
  prStackScreenPage.expandRow("n1");

  // When
  byTestId("work-item-link-session-n1").select(A_SESSION.sessionId);

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(WorkQueueService.method.linkWorkItemSession);
    expect(calls).to.have.length(1);
    expect(calls[0].sessionId).to.equal(A_SESSION.sessionId);
  });
});

it("a PR'ed item shows running, failed and succeeded check counts", () => {
  // When
  cy.mount(<WorkItemChecksSummary itemId="n1" running={1} failed={2} succeeded={3} />);

  // Then
  byTestId("work-item-checks-running-n1").should("have.text", "1");
  byTestId("work-item-checks-failed-n1").should("have.text", "2");
  byTestId("work-item-checks-succeeded-n1").should("have.text", "3");
});
