/**
 * Acceptance (`#workqueue` 11/14): the session inspector's PR view of a session linked to a work item
 * offers Inbox and Outbox beside the pull request, each read through the queue's RPCs; a session no
 * queue links shows the pull request alone.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-11-22-commit-inbox-outbox.md
 */

import React from "react";
import type { Client } from "@connectrpc/connect";
import type { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { SessionPullRequestFlowTabs } from "../../src";

const byTestId = (id: string) => cy.get(`[data-testid="${id}"]`, { timeout: 5000 });

const AN_ADDRESS = { projectId: "p1", queueId: "owner", daemonInstanceId: "host-a" };

/** A queue client whose one queue links session `s-child` to item `n2`. */
function aQueueClientLinking(sessionId: string): Client<typeof WorkQueueService> {
  return {
    listProjectWorkQueues: async () => ({ stacks: [{ address: AN_ADDRESS }] }),
    getWorkQueue: async () => ({
      workQueueJson: JSON.stringify({ nodes: [{ node_id: "n1" }, { node_id: "n2", session_id: sessionId }] }),
    }),
    getWorkItemInbox: async () => ({
      commits: [
        { sha: "x1", subject: "fix one", sourceKind: "item", source: "n1", conflictFiles: ["f.txt"] },
      ],
    }),
    getWorkItemOutbox: async () => ({
      successors: [{ successor: "n3", commits: [{ sha: "y1", subject: "fix two" }] }],
    }),
  } as unknown as Client<typeof WorkQueueService>;
}

function mountFor(sessionId: string) {
  cy.mount(
    <SessionPullRequestFlowTabs
      client={null}
      workQueueClient={aQueueClientLinking("s-child")}
      sessionToken="t"
      projectId="p1"
      sessionId={sessionId}
    />,
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("a session linked to an item offers its inbox, read through the queue", () => {
  // Given
  mountFor("s-child");

  // When
  byTestId("session-pr-flow-tab-inbox").click();

  // Then
  byTestId("pr-inbox-commit-x1").should("have.attr", "data-source", "item:n1").and("have.attr", "data-conflict", "true");
  byTestId("pr-inbox-conflict-files-x1").should("contain.text", "f.txt");
});

it("and its outbox, per successor", () => {
  // Given
  mountFor("s-child");

  // When
  byTestId("session-pr-flow-tab-outbox").click();

  // Then
  byTestId("pr-outbox-commit-n3-y1").should("contain.text", "fix two");
});

it("a session no queue links shows the pull request alone", () => {
  // When
  mountFor("s-unlinked");

  // Then
  byTestId("session-pr-tab").should("be.visible");
  cy.get('[data-testid="session-pr-flow-tab-inbox"]').should("not.exist");
});
