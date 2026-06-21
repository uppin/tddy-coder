import React, { useState } from "react";
import { WorktreesScreen } from "../../src/components/worktrees/WorktreesScreen";
import { worktreesPage } from "../support/pages/worktreesPage";

const MOCK_ROWS = [
  {
    path: "/tmp/accept-wt-main",
    branch: "main",
    sizeLabel: "4096 B",
    changedFiles: 0,
    linesAdded: 0,
    linesRemoved: 0,
  },
  {
    path: "/tmp/accept-wt-detached",
    branch: "(detached)",
    sizeLabel: "8192 B",
    changedFiles: 2,
    linesAdded: 5,
    linesRemoved: 1,
  },
];

function WorktreesHarness() {
  const [deleted, setDeleted] = useState<string | null>(null);
  return (
    <div>
      <nav data-testid="shell-nav">
        <button type="button" data-testid="shell-menu-worktrees">
          Worktrees
        </button>
      </nav>
      <WorktreesScreen worktrees={MOCK_ROWS} onConfirmDelete={(p) => setDeleted(p)} />
      {deleted ? <span data-testid="worktrees-deleted-path">{deleted}</span> : null}
    </div>
  );
}

describe("WorktreesScreen", () => {
  it("renders the menu entry, table headers, and all worktree rows", () => {
    // Given
    cy.mount(<WorktreesHarness />);

    // Then
    worktreesPage.menuButton().should("be.visible");
    worktreesPage.screen().should("exist");
    worktreesPage.table().should("exist");
    cy.contains("th", "Branch").should("be.visible");
    cy.contains("th", "Size").should("be.visible");
    cy.contains("th", "Changed files").should("be.visible");
    worktreesPage.rows().should("have.length", MOCK_ROWS.length);
  });

  it("fires onConfirmDelete with the worktree path after the user clicks Delete then Confirm", () => {
    // Given
    cy.mount(<WorktreesHarness />);

    // When
    worktreesPage.deleteBtn(0).click();
    worktreesPage.confirmDeleteBtn().click();

    // Then
    worktreesPage.deletedPath().should("contain.text", MOCK_ROWS[0].path);
  });
});
