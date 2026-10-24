import React, { useState } from "react";
import { WorktreesScreen } from "../../src/components/worktrees/WorktreesScreen";
import type { WorktreeListItem } from "../../src/lib/worktreeList";
import { worktreesPage as page } from "../support/pages/worktreesPage";

const PROJECTS = [
  { projectId: "alpha", name: "Alpha" },
  { projectId: "bravo", name: "Bravo" },
];

function anItem(overrides: Partial<WorktreeListItem> & { name: string }): WorktreeListItem {
  return {
    path: `/repos/demo/.worktrees/${overrides.name}`,
    branch: "main",
    calculating: false,
    stale: false,
    ...overrides,
  };
}

function Harness({ items }: { items: WorktreeListItem[] }) {
  const [selected, setSelected] = useState<string | null>(items[0]?.path ?? null);
  const [refreshed, setRefreshed] = useState(false);
  const [projectId, setProjectId] = useState("alpha");
  return (
    <div style={{ height: 600, display: "flex" }}>
      <WorktreesScreen
        projects={PROJECTS}
        projectId={projectId}
        onSelectProject={setProjectId}
        items={items}
        selectedPath={selected}
        onSelect={setSelected}
        onRefresh={() => setRefreshed(true)}
        emptyHint="No worktrees for this project yet."
      >
        <span data-testid="pane-for">{selected}</span>
        {refreshed ? <span data-testid="refreshed">yes</span> : null}
      </WorktreesScreen>
    </div>
  );
}

describe("WorktreesScreen", () => {
  beforeEach(() => cy.viewport(1280, 800));

  it("lists worktrees by name, not path, with their disk usage beside them", () => {
    cy.mount(<Harness items={[anItem({ name: "feat-a", diskBytes: 524_288_000n })]} />);

    page.rowName(0).should("have.text", "feat-a");
    page.rowSize(0).should("have.text", "500 MB");
    page.row(0).should("not.contain.text", "/repos/demo");
  });

  it("shows the pane of the worktree that is clicked", () => {
    cy.mount(
      <Harness
        items={[anItem({ name: "big", diskBytes: 9_000n }), anItem({ name: "small", diskBytes: 10n })]}
      />,
    );

    page.row(1).click();

    cy.get("[data-testid=pane-for]").should("have.text", "/repos/demo/.worktrees/small");
  });

  it("offers a project selector and a Refresh that asks for a recalculation of all", () => {
    cy.mount(<Harness items={[anItem({ name: "feat-a", diskBytes: 1n })]} />);

    page.projectSelect().select("bravo").should("have.value", "bravo");
    page.refresh().click();

    cy.get("[data-testid=refreshed]").should("exist");
  });

  it("shows progress and a spinner while sizes are being calculated, and a stale size as such", () => {
    cy.mount(
      <Harness
        items={[
          anItem({ name: "old", diskBytes: 2_048n, stale: true, calculating: true }),
          anItem({ name: "done", diskBytes: 1_024n }),
        ]}
      />,
    );

    page.progress().should("contain.text", "1 of 2");
    page.rowSpinner(0).should("exist");
    page.rowSize(0).should("have.class", "italic");
  });

  it("hides the progress once every size is known", () => {
    cy.mount(<Harness items={[anItem({ name: "done", diskBytes: 1_024n })]} />);

    page.progress().should("not.exist");
  });
});
