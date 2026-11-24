/**
 * Acceptance: the work-queue view stays current while it is open.
 *
 * The view watches its queue (`WatchWorkQueue`) on the host that holds it and shows the newest copy it
 * has seen — a change made anywhere else (an agent's tool, another session) appears without leaving
 * the view, and an older copy arriving late changes nothing. A queue that cannot be loaded says why
 * instead of reading as an empty queue; a dropped watch keeps the queue shown, says it is reconnecting,
 * and is brought up to date by the next snapshot; a removed queue ends the watch for good.
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { WorkQueueView } from "../../src";
import { prStackScreenPage } from "../support/pages/prStackScreenPage";
import { workQueueViewPage } from "../support/pages/workQueueViewPage";
import { aPlannedNode, aWorkQueueJson, type WorkItemFixture } from "../support/rpc/prStackFixtures";
import { aWorkQueueWatchFeed, Code, type WorkQueueWatchFeed } from "../support/rpc/workQueueWatchFeed";

const THE_QUEUE = { projectId: "proj-live", queueId: "0190e000-0000-7000-8000-0000000000aa", daemonInstanceId: "host-b" };
const TOKEN = "fake-token";

const N1 = aPlannedNode({ itemId: "n1", title: "Add token store", displayOrder: 0 });
const N2 = aPlannedNode({ itemId: "n2", title: "Add auth middleware", displayOrder: 1 });
const N3 = aPlannedNode({ itemId: "n3", title: "Add logout", displayOrder: 2 });

function queueAt(revision: number, nodes: WorkItemFixture[]): string {
  return aWorkQueueJson(1, nodes, revision);
}

/**
 * The view, with its unary calls on `viewHost` and its watch on the host that holds the queue,
 * scripted by the returned feed. The view's own host serves no watch: a view watching it would fail.
 */
function theViewWatching(viewHost: InMemoryRpcBackend = anInMemoryRpcBackend()): WorkQueueWatchFeed {
  const feed = aWorkQueueWatchFeed();
  const holdingHost = anInMemoryRpcBackend().implement(WorkQueueService, { watchWorkQueue: feed.impl });
  cy.mountWithClients(
    <WorkQueueView
      address={THE_QUEUE}
      workQueueClient={createClient(WorkQueueService, viewHost.transport())}
      watchClient={createClient(WorkQueueService, holdingHost.transport())}
      sessionToken={TOKEN}
    />,
    viewHost,
  );
  return feed;
}

function subscriptionsOpened(feed: WorkQueueWatchFeed, count: number) {
  cy.wrap(feed).its("subscriptions.length", { timeout: 6000 }).should("equal", count);
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("an item added outside the view appears in it without leaving the view", () => {
  // Given — the view open on a queue holding n1
  const feed = theViewWatching();
  feed.onSubscribe((subscription) => subscription.send(2, queueAt(2, [N1])));
  prStackScreenPage.workItemRowNodeIds().should("deep.equal", ["n1"]);

  // When — an orchestrator's tool adds n2
  cy.then(() => feed.subscriptions[0].send(3, queueAt(3, [N1, N2])));

  // Then — it shows, from the one watch the view opened on the queue's address
  prStackScreenPage.workItemRowNodeIds().should("deep.equal", ["n1", "n2"]);
  cy.wrap(feed).should((f) => {
    expect(f.subscriptions, "one watch").to.have.length(1);
    expect(f.subscriptions[0].request.queue, "on the queue's address").to.include(THE_QUEUE);
    expect(f.subscriptions[0].request.sessionToken).to.equal(TOKEN);
  });
});

it("an older snapshot arriving after the view's own edit does not undo it", () => {
  // Given — the view open on n1, n2; adding a planned PR answers with the queue at revision 4
  const viewHost = anInMemoryRpcBackend().onUnary(WorkQueueService.method.addWorkItem, () => ({
    workQueueJson: queueAt(4, [N1, N2, N3]),
    itemId: "n3",
  }));
  const feed = theViewWatching(viewHost);
  feed.onSubscribe((subscription) => subscription.send(2, queueAt(2, [N1, N2])));
  prStackScreenPage.workItemRowNodeIds().should("deep.equal", ["n1", "n2"]);
  prStackScreenPage.openAddPlannedPrForm();
  prStackScreenPage.fillAndSubmitAddPlannedPrForm({ title: "Add logout" });
  prStackScreenPage.workItemRowNodeIds().should("deep.equal", ["n1", "n2", "n3"]);

  // When — a snapshot from before the edit arrives late
  cy.then(() => feed.subscriptions[0].send(3, queueAt(3, [N1, N2])));
  cy.wrap(feed).should((f) => expect(f.subscriptions[0].consumed, "the view took it").to.equal(2));

  // Then — the view still shows the edit
  prStackScreenPage.workItemRowNodeIds().should("deep.equal", ["n1", "n2", "n3"]);
});

it("a queue that cannot be loaded says why instead of showing an empty queue", () => {
  // Given — the host holding the queue does not answer
  const feed = theViewWatching();
  feed.onSubscribe((subscription) => subscription.fail(Code.Unavailable, "host-b is unreachable"));

  // Then
  workQueueViewPage.loadError().should("contain.text", "host-b is unreachable");
  cy.get("[data-testid^='work-item-row-']").should("not.exist");
});

it("a dropped watch keeps the queue shown, says it is reconnecting, and the next snapshot brings it up to date", () => {
  // Given — the view showing n1
  const feed = theViewWatching();
  feed.onSubscribe((subscription, index) => {
    if (index === 0) subscription.send(2, queueAt(2, [N1]));
  });
  prStackScreenPage.workItemRowNodeIds().should("deep.equal", ["n1"]);

  // When — the watch drops
  cy.then(() => feed.subscriptions[0].fail(Code.Unavailable, "connection reset"));

  // Then — the queue stays, and the view says it is reconnecting
  workQueueViewPage.liveNotice().should("contain.text", "Reconnecting");
  prStackScreenPage.workItemRowNodeIds().should("deep.equal", ["n1"]);

  // When — it resubscribes, and the queue changed meanwhile
  subscriptionsOpened(feed, 2);
  cy.then(() => feed.subscriptions[1].send(3, queueAt(3, [N1, N2])));

  // Then — the resync brings it up to date and the notice goes
  prStackScreenPage.workItemRowNodeIds().should("deep.equal", ["n1", "n2"]);
  workQueueViewPage.liveNotice().should("not.exist");
});

it("a removed queue ends the watch for good and says why", () => {
  // Given — the view showing n1
  const feed = theViewWatching();
  feed.onSubscribe((subscription, index) => {
    if (index === 0) subscription.send(2, queueAt(2, [N1]));
  });
  prStackScreenPage.workItemRowNodeIds().should("deep.equal", ["n1"]);

  // When — the queue's session is deleted
  cy.then(() => feed.subscriptions[0].fail(Code.NotFound, "project proj-live holds no work queue"));

  // Then — the view says so, keeps what it showed, and never watches again
  workQueueViewPage.liveNotice().should("contain.text", "holds no work queue");
  prStackScreenPage.workItemRowNodeIds().should("deep.equal", ["n1"]);
  // Longer than the first reconnect delay: a retry would have opened a second subscription by now.
  cy.wait(2500);
  cy.wrap(feed).its("subscriptions.length").should("equal", 1);
});
