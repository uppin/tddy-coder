/**
 * Acceptance (`#workqueue` 9/14): the queue's graph draws line edges and dependency edges
 * distinguishably, and an external base as an endpoint outside the queue.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-11-22-lineage-graph.md
 */

import React from "react";
import { WorkQueueGraph } from "../../src";

const byTestId = (id: string) => cy.get(`[data-testid="${id}"]`, { timeout: 5000 });

function mountTheGraph() {
  cy.mount(
    <WorkQueueGraph
      items={[
        { itemId: "n1", title: "Token store" },
        { itemId: "n2", title: "Middleware" },
        { itemId: "n3", title: "Logout" },
      ]}
      lineEdges={[
        { itemId: "n1", from: { kind: "trunk", branch: "master" } },
        { itemId: "n2", from: { kind: "item", itemId: "n1" } },
        { itemId: "n3", from: { kind: "external", branch: "feat/other-team" } },
      ]}
      dependencyEdges={[{ from: "n1", to: "n3" }]}
    />,
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("line edges and dependency edges are drawn distinguishably", () => {
  // When
  mountTheGraph();

  // Then
  byTestId("work-queue-graph-line-n1-n2").should("have.attr", "data-kind", "line");
  byTestId("work-queue-graph-dependency-n1-n3").should("have.attr", "data-kind", "dependency");
});

it("an external base is drawn as an endpoint outside the queue", () => {
  // When
  mountTheGraph();

  // Then
  byTestId("work-queue-graph-endpoint-feat/other-team").should("be.visible");
  byTestId("work-queue-graph-endpoint-master").should("be.visible");
  byTestId("work-queue-graph-node-n3").should("be.visible");
});
