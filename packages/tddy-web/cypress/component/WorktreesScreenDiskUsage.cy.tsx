import React from "react";
import { WorktreesScreen } from "../../src/components/worktrees/WorktreesScreen";
import {
  mergeWorktreeItems,
  sortWorktreesBySize,
  type RememberedWorktree,
} from "../../src/lib/worktreeList";
import type { WorktreeStatsRow } from "../../src/lib/worktreeSize";
import { worktreesPage as page } from "../support/pages/worktreesPage";

/**
 * The Worktrees sidebar lists the biggest worktree first, a remembered size stands in (stale) until
 * the daemon's own arrives, and a worktree never sized sinks to the bottom.
 *
 * Feature: docs/ft/web/worktree-disk-usage-streaming.md
 */

function aRow(overrides: Partial<WorktreeStatsRow> & { path: string }): WorktreeStatsRow {
  return {
    branch: "main",
    status: "none",
    changedFiles: 0,
    linesAdded: 0,
    linesRemoved: 0,
    ...overrides,
  };
}

function mountList(streamed: WorktreeStatsRow[], remembered: RememberedWorktree[] = []) {
  const items = sortWorktreesBySize(mergeWorktreeItems(streamed, remembered));
  cy.viewport(1280, 800);
  cy.mount(
    <div style={{ height: 600, display: "flex" }}>
      <WorktreesScreen
        projects={[]}
        projectId=""
        onSelectProject={() => undefined}
        items={items}
        selectedPath={null}
        onSelect={() => undefined}
        onRefresh={() => undefined}
        emptyHint="nothing"
      />
    </div>,
  );
}

describe("Worktrees sidebar disk usage", () => {
  it("sorts biggest first, with the never-sized last", () => {
    mountList([
      aRow({ path: "/w/none" }),
      aRow({ path: "/w/small", status: "cached", diskBytes: 1_024n }),
      aRow({ path: "/w/big", status: "cached", diskBytes: 1_288_490_189n }),
    ]);

    page.rowName(0).should("have.text", "big");
    page.rowSize(0).should("have.text", "1.2 GB");
    page.rowName(1).should("have.text", "small");
    page.rowName(2).should("have.text", "none");
  });

  it("shows the remembered size at once, before the daemon has answered", () => {
    mountList([], [{ path: "/w/a", branch: "main", diskBytes: 524_288_000n }]);

    page.rowSize(0).should("have.text", "500 MB").and("have.class", "italic");
    page.progress().should("not.exist");
  });

  it("keeps the remembered size beside the spinner while a fresh one is calculated", () => {
    mountList(
      [aRow({ path: "/w/a", status: "calculating" })],
      [{ path: "/w/a", branch: "main", diskBytes: 524_288_000n }],
    );

    page.rowSize(0).should("have.text", "500 MB");
    page.rowSpinner(0).should("exist");
  });
});
