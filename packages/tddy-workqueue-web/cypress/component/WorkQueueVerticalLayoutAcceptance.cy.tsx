/**
 * Acceptance: the work queue reads top-to-bottom, one task box per row, with a small gap between
 * boxes, a drag handle at each task's left edge, and a bracket on the right gutter wherever two
 * tasks are linked by a PR-stack line edge. Tasks bound to a PR stack cannot be dragged; free tasks
 * can, and a drop persists through the existing `ReorderWorkItem` RPC.
 *
 * Changeset: docs/dev/1-WIP/2026-11-23-workqueue-sidebar-ux.md
 */

import React from "react";
import { createClient } from "@connectrpc/connect";
import { create } from "@bufbuild/protobuf";
import { anInMemoryRpcBackend, type InMemoryRpcBackend } from "tddy-connectrpc-testkit";
import { WorkQueueService } from "tddy-client-web/gen/work_queue_pb";
import { WorkQueueView } from "../../src";
import { aPlannedNode, aWorkQueueJson } from "../support/rpc/prStackFixtures";

const ADDRESS = { projectId: "proj-ux", queueId: "queue-ux", daemonInstanceId: "" };

// n1 and n2 sit on a PR-stack line (n2's PR base is n1's branch); n3 is free; n4 has a branch but no line.
const NODES = [
  aPlannedNode({ itemId: "n1", title: "Token store", branch: "feat/token-store", displayOrder: 0 }),
  aPlannedNode({ itemId: "n2", title: "Middleware", branch: "feat/middleware", parents: ["n1"], displayOrder: 1 }),
  aPlannedNode({ itemId: "n3", title: "Free task", displayOrder: 2 }),
  aPlannedNode({ itemId: "n4", title: "Lone branch", branch: "feat/lone", displayOrder: 3 }),
];

const handle = (id: string) => cy.get(`[data-testid="work-item-drag-handle-${id}"]`, { timeout: 5000 });
const row = (id: string) => cy.get(`[data-testid="work-item-row-${id}"]`, { timeout: 5000 });

function theQueue(): InMemoryRpcBackend {
  const backend = anInMemoryRpcBackend()
    .onUnary(WorkQueueService.method.getWorkQueue, () =>
      create(WorkQueueService.method.getWorkQueue.output, { workQueueJson: aWorkQueueJson(1, NODES) }),
    )
    .onUnary(WorkQueueService.method.getWorkQueueGraph, () =>
      create(WorkQueueService.method.getWorkQueueGraph.output, {
        lineEdges: [
          { itemId: "n1", kind: "trunk", endpoint: "master" },
          { itemId: "n2", kind: "item", endpoint: "n1" },
        ],
      }),
    )
    .onUnary(WorkQueueService.method.reorderWorkItem, () =>
      create(WorkQueueService.method.reorderWorkItem.output, { workQueueJson: aWorkQueueJson(1, NODES) }),
    );
  cy.mountWithClients(
    <div style={{ width: 420, height: 700, display: "flex", flexDirection: "column" }}>
      <WorkQueueView
        address={ADDRESS}
        workQueueClient={createClient(WorkQueueService, backend.transport())}
        sessionToken="fake-token"
      />
    </div>,
    backend,
  );
  return backend;
}

/** Drag `id`'s handle to the vertical middle of row `targetId`'s lower half or upper half. */
function dragHandleTo(id: string, targetId: string, where: "above" | "below", expectDragging = true) {
  handle(id).then(($h) => {
    const from = $h[0].getBoundingClientRect();
    row(targetId).then(($t) => {
      const to = $t[0].getBoundingClientRect();
      const y = where === "above" ? to.top + 4 : to.bottom - 4;
      const x = from.left + from.width / 2;
      cy.wrap($h).trigger("pointerdown", { button: 0, pointerId: 1, clientX: x, clientY: from.top + from.height / 2, force: true });
      // a free task's handle reports the drag once React has rendered it; a bound one never does
      if (expectDragging) cy.get('[data-dragging="true"]', { timeout: 1000 }).should("have.length", 1);
      else cy.wait(300);
      cy.document().trigger("pointermove", { pointerId: 1, clientX: x, clientY: y });
      // the grabbed box follows the pointer and a drop line shows where it would land
      if (expectDragging) cy.get('[data-testid="work-item-drop-indicator"]').should("exist");
      cy.document().trigger("pointerup", { pointerId: 1, clientX: x, clientY: y });
    });
  });
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("tasks stack top-to-bottom, one box per row, and never scroll sideways", () => {
  // When
  theQueue();

  // Then
  row("n1").then(($a) => row("n2").then(($b) => {
    const a = $a[0].getBoundingClientRect();
    const b = $b[0].getBoundingClientRect();
    expect(b.top, "second box is below the first").to.be.at.least(a.bottom);
    expect(b.left, "same column").to.be.closeTo(a.left, 1);
  }));
  cy.get('[data-testid="work-item-list"]').should(($list) => {
    expect($list[0].scrollWidth, "no horizontal overflow").to.be.at.most($list[0].clientWidth);
  });
  cy.get('[data-testid="work-queue-graph"]').should("not.exist");
});

it("a small gap separates the task boxes", () => {
  // When
  theQueue();

  // Then
  row("n1").then(($a) => row("n2").then(($b) => {
    const gap = $b[0].getBoundingClientRect().top - $a[0].getBoundingClientRect().bottom;
    expect(gap).to.be.within(4, 24);
  }));
});

it("every task has a drag handle at its left edge", () => {
  // When
  theQueue();

  // Then
  for (const id of ["n1", "n2", "n3", "n4"]) {
    handle(id).then(($h) => row(id).then(($r) => {
      const h = $h[0].getBoundingClientRect();
      const r = $r[0].getBoundingClientRect();
      expect(h.left - r.left, `handle of ${id} is at the left edge`).to.be.within(0, 24);
      expect(h.right).to.be.at.most(r.left + 40);
    }));
  }
});

it("two tasks linked by a PR-stack line edge are joined by a bracket on their right sides", () => {
  // When
  theQueue();

  // Then
  cy.get('[data-testid="work-item-connector-n1-n2"]', { timeout: 5000 }).should("exist").then(($c) => {
    row("n1").then(($a) => row("n2").then(($b) => {
      const c = $c[0].getBoundingClientRect();
      const a = $a[0].getBoundingClientRect();
      const b = $b[0].getBoundingClientRect();
      expect(c.left, "starts at the right edge of the boxes").to.be.at.least(a.right - 2);
      expect(c.top, "starts in the parent's vertical range").to.be.within(a.top, a.bottom);
      expect(c.bottom, "ends in the child's vertical range").to.be.within(b.top, b.bottom);
      expect(c.right, "bulges into the gutter").to.be.greaterThan(a.right + 2);
    }));
  });
  // the trunk and free or lone tasks draw no connector between boxes
  cy.get('[data-testid^="work-item-connector-"]').should("have.length", 1);
});

it("a task bound to a PR stack has a disabled handle that says why", () => {
  // When
  const backend = theQueue();

  // Then
  for (const id of ["n1", "n2", "n4"]) {
    handle(id).should("be.disabled").and("have.attr", "title").and("match", /PR stack/i);
  }

  // When — a drag is attempted anyway
  dragHandleTo("n2", "n1", "above", false);

  // Then
  cy.wait(200).then(() => {
    expect(backend.callsTo(WorkQueueService.method.reorderWorkItem)).to.have.length(0);
  });
});

it("a free task can be dragged between stack items and the move is persisted as a reorder", () => {
  // Given
  const backend = theQueue();
  handle("n3").should("not.be.disabled");

  // When — n3 moves from below n2 to between n1 and n2
  dragHandleTo("n3", "n2", "above");

  // Then
  cy.wrap(backend).should((b) => {
    const calls = b.callsTo(WorkQueueService.method.reorderWorkItem);
    expect(calls).to.have.length(1);
    expect(calls[0].itemId).to.equal("n3");
    expect(calls[0].direction).to.equal("up");
    expect(calls[0].stack).to.include(ADDRESS);
  });
});
