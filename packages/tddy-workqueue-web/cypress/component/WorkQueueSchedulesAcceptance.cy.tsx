/**
 * Acceptance (`#workqueue` 13/14): the schedules panel lists each armed schedule's conditions with
 * their state and detail, and follows a condition turning met without a reload.
 *
 * Feature: docs/ft/coder/pr-stacking.md § Armed merges and propagations
 */

import React from "react";
import { SchedulesPanel, type ScheduleViewModel } from "../../src";

const byTestId = (id: string) => cy.get(`[data-testid="${id}"]`, { timeout: 5000 });

function aMergeWaitingOnChecks(checks: "waiting" | "met"): ScheduleViewModel {
  return {
    itemId: "n2",
    kind: "merge",
    status: checks,
    heldUpOn: checks === "waiting" ? "checks" : undefined,
    conditions: [
      { name: "order", state: "met", detail: "based on master" },
      { name: "current", state: "met", detail: "inbox empty" },
      { name: "checks", state: checks, detail: checks === "waiting" ? "2 running" : "all green" },
      { name: "review", state: "met", detail: "approved by octocat" },
    ],
  };
}

it("the panel lists each schedule's conditions with their state and detail", () => {
  // When
  cy.mount(<SchedulesPanel schedules={[aMergeWaitingOnChecks("waiting")]} />);

  // Then
  byTestId("work-queue-schedule-n2").should("have.attr", "data-status", "waiting");
  byTestId("work-queue-schedule-n2-checks").should("have.attr", "data-state", "waiting").and("contain.text", "2 running");
  byTestId("work-queue-schedule-n2-held-up-on").should("contain.text", "checks");
});

it("a condition turning met updates the panel without a reload", () => {
  // Given
  cy.mount(<SchedulesPanel schedules={[aMergeWaitingOnChecks("waiting")]} />).then(({ rerender }) => {
    // When — the status stream delivers the next view
    rerender(<SchedulesPanel schedules={[aMergeWaitingOnChecks("met")]} />);
  });

  // Then
  byTestId("work-queue-schedule-n2").should("have.attr", "data-status", "met");
  byTestId("work-queue-schedule-n2-checks").should("contain.text", "all green");
});
