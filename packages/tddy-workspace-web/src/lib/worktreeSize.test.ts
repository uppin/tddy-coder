import { describe, expect, it } from "bun:test";
import {
  applyWorktreeStatsEvent,
  formatLastCalculated,
  type WorktreeStatsRow,
} from "./worktreeSize";

// A fixed "now" so the relative-time cases are deterministic.
const NOW = 1_700_000_000_000;

function aRow(overrides: Partial<WorktreeStatsRow> & { path: string }): WorktreeStatsRow {
  return {
    branch: "feature/x",
    status: "none",
    changedFiles: 0,
    linesAdded: 0,
    linesRemoved: 0,
    ...overrides,
  };
}

describe("formatLastCalculated — relative time for a cached worktree", () => {
  it("reads 'never' when there is no calculation timestamp", () => {
    expect(formatLastCalculated(undefined, NOW)).toBe("never");
  });

  it("reads 'just now' within the first minute", () => {
    expect(formatLastCalculated(NOW - 30_000, NOW)).toBe("just now");
  });

  it("reads whole minutes for a sub-hour age", () => {
    expect(formatLastCalculated(NOW - 120_000, NOW)).toBe("2 min ago");
  });

  it("reads whole hours for a sub-day age", () => {
    expect(formatLastCalculated(NOW - 7_200_000, NOW)).toBe("2 hr ago");
  });

  it("reads whole days beyond a day", () => {
    expect(formatLastCalculated(NOW - 259_200_000, NOW)).toBe("3 d ago");
  });
});

describe("applyWorktreeStatsEvent — folding the worktree-stats stream into rows", () => {
  it("replaces the row set with a snapshot event's rows", () => {
    // Given some existing rows
    const existing = [aRow({ path: "/wt/a" })];

    // When a snapshot frame arrives
    const next = applyWorktreeStatsEvent(existing, {
      snapshot: [
        aRow({ path: "/wt/x", status: "none" }),
        aRow({ path: "/wt/y", status: "calculating" }),
      ],
    });

    // Then the rows become exactly the snapshot
    expect(next.map((r) => r.path)).toEqual(["/wt/x", "/wt/y"]);
    expect(next.map((r) => r.status)).toEqual(["none", "calculating"]);
  });

  it("patches only the matching worktree on an update, preserving order and siblings", () => {
    // Given two known worktrees
    const rows = [
      aRow({ path: "/wt/a", status: "none" }),
      aRow({ path: "/wt/b", status: "calculating" }),
    ];

    // When one worktree finishes calculating
    const next = applyWorktreeStatsEvent(rows, {
      updated: aRow({
        path: "/wt/b",
        status: "cached",
        diskBytes: 1288490189n,
        calculatedAtUnixMs: NOW,
      }),
    });

    // Then only that row changes; the other is untouched and order is preserved
    expect(next.map((r) => r.path)).toEqual(["/wt/a", "/wt/b"]);
    expect(next[0].status).toBe("none");
    expect(next[1].status).toBe("cached");
    expect(next[1].diskBytes).toBe(1288490189n);
  });

  it("appends a worktree that is not yet in the rows on an update", () => {
    // Given one known worktree
    const rows = [aRow({ path: "/wt/a" })];

    // When an update arrives for a worktree not in the list
    const next = applyWorktreeStatsEvent(rows, {
      updated: aRow({ path: "/wt/c", status: "calculating" }),
    });

    // Then it is appended
    expect(next.map((r) => r.path)).toEqual(["/wt/a", "/wt/c"]);
  });
});
