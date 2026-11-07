import { describe, expect, it } from "bun:test";
import {
  mergeWorktreeItems,
  sizingProgress,
  sortWorktreesBySize,
  worktreeName,
  type WorktreeListItem,
} from "./worktreeList";
import type { WorktreeStatsRow } from "./worktreeSize";

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

function anItem(overrides: Partial<WorktreeListItem> & { name: string }): WorktreeListItem {
  return { path: `/w/${overrides.name}`, branch: "main", calculating: false, stale: false, ...overrides };
}

describe("worktreeName", () => {
  it("is the directory name, not the path", () => {
    expect(worktreeName("/repo/.worktrees/feat-x/")).toBe("feat-x");
  });
});

describe("sortWorktreesBySize", () => {
  it("lists the biggest first and the never-sized last", () => {
    const sorted = sortWorktreesBySize([
      anItem({ name: "unsized" }),
      anItem({ name: "small", diskBytes: 10n }),
      anItem({ name: "big", diskBytes: 9000n }),
    ]);
    expect(sorted.map((i) => i.name)).toEqual(["big", "small", "unsized"]);
  });
});

describe("mergeWorktreeItems", () => {
  const remembered = [{ path: "/w/a", branch: "main", diskBytes: 500n, calculatedAtUnixMs: 1 }];

  it("lists remembered worktrees as stale before the daemon answers", () => {
    const items = mergeWorktreeItems([], remembered);
    expect(items).toMatchObject([{ name: "a", diskBytes: 500n, stale: true, calculating: false }]);
  });

  it("keeps the remembered size, flagged stale, while a fresh one is calculated", () => {
    const items = mergeWorktreeItems([aRow({ path: "/w/a", status: "calculating" })], remembered);
    expect(items).toMatchObject([{ diskBytes: 500n, stale: true, calculating: true }]);
  });

  it("replaces the remembered size once the daemon has a fresh one", () => {
    const row = aRow({ path: "/w/a", status: "cached", diskBytes: 700n, calculatedAtUnixMs: 2 });
    expect(mergeWorktreeItems([row], remembered)).toMatchObject([
      { diskBytes: 700n, stale: false, calculating: false },
    ]);
  });
});

describe("sizingProgress", () => {
  it("counts the worktrees no longer calculating", () => {
    const items = [anItem({ name: "a" }), anItem({ name: "b", calculating: true })];
    expect(sizingProgress(items)).toEqual({ done: 1, total: 2 });
  });
});
