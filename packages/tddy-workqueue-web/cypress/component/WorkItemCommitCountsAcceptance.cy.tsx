/**
 * Acceptance (`#workqueue` 11/14): a queue row shows what its item is missing and owes, with a
 * conflict marker, and nothing for an item with neither.
 *
 * Feature: docs/ft/coder/pr-stacking.md § Commit inbox and outbox
 */

import React from "react";
import { WorkItemCommitCounts, WorkItemCommitCountsContext } from "../../src";

const byTestId = (id: string) => cy.get(`[data-testid="${id}"]`, { timeout: 5000 });

function mountRowsWith(counts: Record<string, { inbox: number; outbox: number; conflicts: number }>) {
  cy.mount(
    <WorkItemCommitCountsContext.Provider value={new Map(Object.entries(counts))}>
      <WorkItemCommitCounts itemId="n2" />
      <WorkItemCommitCounts itemId="n3" />
    </WorkItemCommitCountsContext.Provider>,
  );
}

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("the row shows its inbox, outbox and conflict counts", () => {
  // When
  mountRowsWith({ n2: { inbox: 3, outbox: 1, conflicts: 2 } });

  // Then
  byTestId("work-item-commit-counts-n2")
    .should("have.attr", "data-inbox", "3")
    .and("have.attr", "data-outbox", "1")
    .and("have.attr", "data-conflicts", "2")
    .and("contain.text", "⚠2");
});

it("an item with nothing to take or give shows no counts", () => {
  // When
  mountRowsWith({ n3: { inbox: 0, outbox: 0, conflicts: 0 } });

  // Then
  cy.get('[data-testid="work-item-commit-counts-n3"]').should("not.exist");
});
