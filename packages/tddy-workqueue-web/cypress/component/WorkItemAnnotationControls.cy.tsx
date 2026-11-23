/**
 * `#workqueue` 7/14: a row's annotation controls — edit the labels as a set, clear a badge by key.
 */

import React from "react";
import { WorkItemAnnotationControls } from "../../src";
import { WorkItemLinksContext, type WorkItemLinks } from "../../src/workItemLinks";
import { parseWorkQueue } from "../../src/workQueue";
import { aPlannedNode, aWorkQueueJson } from "../support/rpc/prStackFixtures";

const AN_ANNOTATED_ITEM = {
  ...aPlannedNode({ itemId: "n1", title: "Wire the API", displayOrder: 0 }),
  labels: ["api", "needs-review"],
  badges: [{ key: "qa", text: "passed", tone: "success" }],
};

const byTestId = (id: string) => cy.get(`[data-testid="${id}"]`);

function mountWith(overrides: Partial<WorkItemLinks>) {
  const [item] = parseWorkQueue(aWorkQueueJson(1, [AN_ANNOTATED_ITEM as never])).nodes;
  const links: WorkItemLinks = {
    sessions: [],
    checksByItemId: {},
    errorByItemId: {},
    onAssignPr: () => {},
    onUnassignPr: () => {},
    onLinkSession: () => {},
    onUnlinkSession: () => {},
    onSetLabels: () => {},
    onClearBadge: () => {},
    ...overrides,
  };
  cy.mount(
    <WorkItemLinksContext.Provider value={links}>
      <WorkItemAnnotationControls node={item} />
    </WorkItemLinksContext.Provider>,
  );
}

it("editing the labels saves the whole set, trimmed and without blanks", () => {
  // Given
  const saved = cy.stub().as("onSetLabels");
  mountWith({ onSetLabels: saved });

  // When
  byTestId("work-item-edit-labels-n1").click();
  byTestId("work-item-edit-labels-input-n1").should("have.value", "api, needs-review").clear().type("api,  ui , ,done");
  byTestId("work-item-edit-labels-save-n1").click();

  // Then
  cy.get("@onSetLabels").should("have.been.calledOnceWith", "n1", ["api", "ui", "done"]);
});

it("clearing a badge asks to remove that key only", () => {
  // Given
  const cleared = cy.stub().as("onClearBadge");
  mountWith({ onClearBadge: cleared });

  // When
  byTestId("work-item-clear-badge-n1-qa").click();

  // Then
  cy.get("@onClearBadge").should("have.been.calledOnceWith", "n1", "qa");
});
