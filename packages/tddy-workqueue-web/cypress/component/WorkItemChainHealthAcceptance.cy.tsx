/**
 * Acceptance (`#workqueue` 12/14): a line's health is icons — a leak icon naming the ancestor and a
 * linearity icon — whose tooltips carry the detail.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-11-22-leak-linearity.md
 */

import React from "react";
import { WorkItemChainHealth } from "../../src";

const byTestId = (id: string) => cy.get(`[data-testid="${id}"]`, { timeout: 5000 });

it("a line shows a leak icon naming the ancestor and a linearity icon", () => {
  // When
  cy.mount(
    <WorkItemChainHealth
      itemId="n3"
      leaks={[
        { sha: "p1", ancestor: "feat/two" },
        { sha: "p2", ancestor: "feat/two" },
      ]}
      line={{ state: "behind", by: 4 }}
    />,
  );

  // Then
  byTestId("work-item-leaks-n3").should("have.attr", "title").and("match", /2 leaked from feat\/two/);
  byTestId("work-item-linearity-n3").should("have.attr", "data-state", "behind").and("have.attr", "title", "Line: behind 4");
});
