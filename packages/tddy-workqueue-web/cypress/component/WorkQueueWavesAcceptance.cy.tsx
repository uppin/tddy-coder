/**
 * Acceptance (`#workqueue` 10/14): a queue's items drawn in wave lanes with their readiness, and a
 * banner naming the first item out of place when the line steps back a wave.
 *
 * PRD: docs/ft/coder/1-WIP/PRD-2026-11-22-waves-line.md
 */

import React from "react";
import { WaveLanes } from "../../src";

const byTestId = (id: string) => cy.get(`[data-testid="${id}"]`, { timeout: 5000 });

const THE_WAVES = [
  [{ itemId: "n1", title: "Token store", waitsOn: [] }],
  [
    { itemId: "n2", title: "Middleware", waitsOn: ["n1"] },
    { itemId: "n3", title: "Logout", waitsOn: [] },
  ],
];

beforeEach(() => {
  cy.viewport(1280, 800);
});

it("items are drawn in wave lanes with their readiness", () => {
  // When
  cy.mount(<WaveLanes waves={THE_WAVES} />);

  // Then
  byTestId("work-queue-wave-1-item-n1").should("be.visible");
  byTestId("work-queue-wave-2-item-n2").should("be.visible");
  byTestId("work-item-readiness-n2").should("contain.text", "waits on n1");
  byTestId("work-item-readiness-n3").should("contain.text", "ready");
  cy.get('[data-testid="work-queue-line-check"]').should("not.exist");
});

it("an out-of-order line shows the banner naming the first item out of place", () => {
  // When
  cy.mount(<WaveLanes waves={THE_WAVES} outOfOrderAt="n3" />);

  // Then
  byTestId("work-queue-line-check").should("contain.text", "n3");
});
