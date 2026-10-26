/**
 * The `ListWorktrees` exec tool lists worktrees in the order the Worktrees screen does
 * (`#start-parity` 3/9).
 *
 * The screen's own `sortWorktreesBySize` is run over the fixture the tool's Rust test sorts too
 * (`tddy-tool-engine/tests/worktree_tools_parity.rs`), so both are held to one expected order. The
 * fixture's mixed-case names pin the screen's `localeCompare` tie-break.
 *
 * Pure-function spec: nothing is mounted.
 *
 * Changeset: docs/dev/1-WIP/2026-10-25-start-parity-worktree-list.md
 */

import { sortWorktreesBySize, type WorktreeListItem } from "../../src/lib/worktreeList";
import fixture from "../support/fixtures/worktree-sort-order.json";

function asScreenItem(row: { name: string; disk_bytes: number | null }): WorktreeListItem {
  return {
    path: `/repo/.worktrees/${row.name}`,
    name: row.name,
    branch: row.name,
    diskBytes: row.disk_bytes === null ? undefined : BigInt(row.disk_bytes),
    calculating: false,
    stale: false,
  };
}

describe("ListWorktrees tool parity with the Worktrees screen", () => {
  it("is ordered as the screen orders worktrees", () => {
    // When the screen's sort runs over the shared fixture
    const sorted = sortWorktreesBySize(fixture.input.map(asScreenItem)).map((item) => item.name);

    // Then it is the order the tool is held to
    expect(sorted).to.deep.equal(fixture.sorted);
  });
});
