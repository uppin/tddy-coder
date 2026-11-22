/**
 * Acceptance (`#workqueue` 14/14): Land wave asks for the wave range and shows the result per item; a
 * refused propagation shows the state and the actions that would fix it.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-11-22-land-propagate.md
 */

import React from "react";
import { LandWaves, PropagationRefusal } from "../../src";

const byTestId = (id: string) => cy.get(`[data-testid="${id}"]`, { timeout: 5000 });

it("Land wave asks for the wave range and shows the result per item", () => {
  // Given
  const onLand = cy.stub().as("land");
  cy.mount(
    <LandWaves
      waveCount={3}
      onLand={onLand}
      results={[
        { itemId: "n1", outcome: "landed", detail: "merged as abc123" },
        { itemId: "n2", outcome: "stopped", detail: "checks: 2 running" },
      ]}
    />,
  );

  // When
  byTestId("work-queue-land-btn").click();
  byTestId("work-queue-land-from").select("1");
  byTestId("work-queue-land-to").select("2");
  byTestId("work-queue-land-submit").click();

  // Then
  cy.get("@land").should("have.been.calledOnceWith", 1, 2);
  byTestId("work-queue-land-result-n1").should("have.attr", "data-outcome", "landed");
  byTestId("work-queue-land-result-n2").should("have.attr", "data-outcome", "stopped").and("contain.text", "checks");
});

it("a refused propagation shows the state and the actions that would fix it", () => {
  // When
  cy.mount(<PropagationRefusal itemId="n2" state="diverged" actions={["pull", "push"]} />);

  // Then
  byTestId("work-item-propagation-refusal-n2")
    .should("contain.text", "diverged")
    .and("contain.text", "pull")
    .and("contain.text", "push");
});
