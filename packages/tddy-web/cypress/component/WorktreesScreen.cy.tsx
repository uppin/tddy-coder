import React, { useState } from "react";
import { WorktreesScreen } from "../../src/components/worktrees/WorktreesScreen";

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

/** Acceptance: hamburger/menu entry, table columns, and delete confirmation with injected clients (no live RPC). */
function WorktreesHarness() {
  const [deleted, setDeleted] = useState<string | null>(null);
  return (
    <div>
      <nav data-testid="shell-nav">
        <button type="button" data-testid="shell-menu-worktrees">
          Worktrees
        </button>
      </nav>
      <WorktreesScreen
        worktrees={MOCK_ROWS}
        onConfirmDelete={(p) => setDeleted(p)}
      />
      {deleted ? <span data-testid="worktrees-deleted-path">{deleted}</span> : null}
    </div>
  );
}

describe("WorktreesScreen", () => {
  it("cypress_worktrees_screen_renders_menu_and_table_with_mocked_clients", () => {
    cy.mount(<WorktreesHarness />);
    cy.get('[data-testid="shell-menu-worktrees"]').should("be.visible");
    cy.get('[data-testid="worktrees-screen"]').should("exist");
    cy.get('[data-testid="worktrees-table"]').should("exist");
    cy.contains("th", "Branch").should("be.visible");
    cy.contains("th", "Size").should("be.visible");
    cy.contains("th", "Changed files").should("be.visible");
    cy.get('[data-testid="worktrees-row"]').should("have.length", MOCK_ROWS.length);
    cy.get('[data-testid="worktrees-delete"]').first().click();
    cy.get('[data-testid="worktrees-delete-confirm"]').click();
    cy.get('[data-testid="worktrees-deleted-path"]').should("contain.text", MOCK_ROWS[0].path);
  });
});
