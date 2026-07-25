import React, { useState } from "react";
import { WorktreesScreen } from "../../src/components/worktrees/WorktreesScreen";
import { worktreesPage as page } from "../support/pages/worktreesPage";

/**
 * Lazy, streamed worktree disk usage — the Worktrees screen surfaces each worktree's size status
 * (None / Calculating / Cached with a last-calculated label), a project-wide Recalculate-all
 * control, and a per-row Calculate control.
 *
 * Feature: docs/ft/web/worktree-disk-usage-streaming.md
 */

const NONE_ROW = {
  path: "/repos/demo/.worktrees/feat-a",
  branch: "feature/a",
  status: "none" as const,
  changedFiles: 0,
  linesAdded: 0,
  linesRemoved: 0,
};

const CALCULATING_ROW = {
  path: "/repos/demo/.worktrees/feat-b",
  branch: "feature/b",
  status: "calculating" as const,
  changedFiles: 2,
  linesAdded: 5,
  linesRemoved: 1,
};

const CACHED_ROW = {
  path: "/repos/demo/.worktrees/feat-c",
  branch: "feature/c",
  status: "cached" as const,
  sizeLabel: "1.2 GB",
  lastCalculatedLabel: "2 min ago",
  changedFiles: 7,
  linesAdded: 240,
  linesRemoved: 18,
};

const ROWS = [NONE_ROW, CALCULATING_ROW, CACHED_ROW];

function WorktreesHarness() {
  const [calculatedPath, setCalculatedPath] = useState<string | null>(null);
  const [recalculatedAll, setRecalculatedAll] = useState(false);
  return (
    <div>
      <WorktreesScreen
        worktrees={ROWS as never}
        onCalculate={(p: string) => setCalculatedPath(p)}
        onRecalculateAll={() => setRecalculatedAll(true)}
      />
      {calculatedPath ? (
        <span data-testid="worktrees-calculated-path">{calculatedPath}</span>
      ) : null}
      {recalculatedAll ? <span data-testid="worktrees-recalculated-all">yes</span> : null}
    </div>
  );
}

describe("Worktrees screen — lazy disk usage", () => {
  it("shows the size status for each worktree", () => {
    // Given the screen rendering a None, a Calculating, and a Cached worktree
    cy.mount(<WorktreesHarness />);

    // Then each row shows its status
    page.status(0).should("contain.text", "None");
    page.status(1).should("contain.text", "Calculating");
    page.status(2).should("contain.text", "Cached");
  });

  it("shows no size and offers Calculate for a never-calculated worktree", () => {
    // Given the screen
    cy.mount(<WorktreesHarness />);

    // Then the None row shows no byte size and offers a Calculate action
    page.row(0).should("not.contain.text", "GB");
    page.row(0).should("not.contain.text", " B");
    page.calculateBtn(0).should("be.visible");
  });

  it("shows a calculating worktree without a size yet", () => {
    // Given the screen
    cy.mount(<WorktreesHarness />);

    // Then the Calculating row reports Calculating and shows no byte size
    page.status(1).should("contain.text", "Calculating");
    page.row(1).should("not.contain.text", "GB");
  });

  it("shows the size and last-calculated time for a cached worktree", () => {
    // Given the screen
    cy.mount(<WorktreesHarness />);

    // Then the Cached row shows its formatted size and when it was last calculated
    page.row(2).should("contain.text", "1.2 GB");
    page.lastCalculated(2).should("contain.text", "2 min ago");
  });

  it("triggers a project-wide recalculation from Recalculate all", () => {
    // Given the screen
    cy.mount(<WorktreesHarness />);

    // When Recalculate all is pressed
    page.recalculateAll().click();

    // Then the project-wide recalculation is requested
    page.recalculatedAll().should("exist");
  });

  it("triggers calculation for a single worktree's path from its Calculate button", () => {
    // Given the screen
    cy.mount(<WorktreesHarness />);

    // When the first row's Calculate is pressed
    page.calculateBtn(0).click();

    // Then calculation is requested for exactly that worktree's path
    page.calculatedPath().should("have.text", NONE_ROW.path);
  });
});
