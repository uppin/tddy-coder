import { describe, expect, it } from "bun:test";
import { create } from "@bufbuild/protobuf";
import { BranchBaseSyncSchema } from "../../../gen/connection_pb";
import { aBranchResolution } from "../../../test-utils";
import { baseSyncView, canPullFromBase } from "./baseSyncStatus";

/**
 * Tests for `baseSyncView` — how a planned-PR row states its branch's standing against its base.
 *
 * Two conflations are the reason this is a discriminated view rather than a few booleans read
 * straight off the wire:
 *
 * - **A comparison that could not be made is not "clean".** It arrives with no commits behind and no
 *   conflicts, byte-identical to a healthy branch, so only its own discriminator can tell them
 *   apart. This is the rule that already governs PR status, and conflating the two is exactly how a
 *   live open PR stayed invisible while the daemon held no GitHub credential.
 * - **A comparison that has not arrived is not "clean" either.** An unanswered poll and a legacy
 *   daemon that sends no comparison at all are both "unknown", and a row says nothing it does not
 *   know.
 *
 * `canPullFromBase` is the gate on offering the pull controls: an action derived from a comparison
 * that was never made is an action derived from nothing.
 */

const BASE = "origin/master";

/** A resolution carrying a base comparison with only the fields a scenario cares about. */
function aResolutionWithBaseSync(
  baseSync: Partial<Parameters<typeof create<typeof BranchBaseSyncSchema>>[1]>,
) {
  return aBranchResolution({
    baseSync: create(BranchBaseSyncSchema, {
      baseBranch: BASE,
      behindCount: 0,
      aheadCount: 0,
      hasConflicts: false,
      conflictedPaths: [],
      unavailable: false,
      unavailableReason: "",
      ...baseSync,
    }),
  });
}

describe("baseSyncView", () => {
  it("reports unknown while the branch resolution has not arrived", () => {
    // Given / When
    const view = baseSyncView(undefined);

    // Then
    expect(view.kind).toBe("unknown");
  });

  it("reports unknown for a resolution that carries no comparison at all", () => {
    // Given — a daemon that predates base sync answers with its other legs only
    const resolution = aBranchResolution();

    // When
    const view = baseSyncView(resolution);

    // Then
    expect(view.kind).toBe("unknown");
  });

  it("reports unavailable with the daemon's reason when the comparison could not be made", () => {
    // Given
    const resolution = aResolutionWithBaseSync({
      unavailable: true,
      unavailableReason: "base branch 'origin/master' resolves to no commit",
    });

    // When
    const view = baseSyncView(resolution);

    // Then
    expect(view).toEqual({
      kind: "unavailable",
      reason: "base branch 'origin/master' resolves to no commit",
    });
  });

  it("reports unavailable rather than in sync when a failed comparison reports nothing behind", () => {
    // Given — the failure case is byte-identical to a healthy branch on every other field
    const resolution = aResolutionWithBaseSync({
      behindCount: 0,
      hasConflicts: false,
      unavailable: true,
      unavailableReason: "not a git repository",
    });

    // When
    const view = baseSyncView(resolution);

    // Then
    expect(view.kind).toBe("unavailable");
  });

  it("reports conflicts when the branch cannot be merged with its base", () => {
    // Given
    const resolution = aResolutionWithBaseSync({
      behindCount: 4,
      hasConflicts: true,
      conflictedPaths: ["src/auth/mod.rs"],
    });

    // When
    const view = baseSyncView(resolution);

    // Then
    expect(view).toEqual({
      kind: "conflicts",
      baseBranch: BASE,
      paths: ["src/auth/mod.rs"],
    });
  });

  it("reports conflicts even when the branch is not behind its base", () => {
    // Given — the behind count must not be what decides whether the operator is told
    const resolution = aResolutionWithBaseSync({
      behindCount: 0,
      hasConflicts: true,
      conflictedPaths: ["src/auth/mod.rs"],
    });

    // When
    const view = baseSyncView(resolution);

    // Then
    expect(view.kind).toBe("conflicts");
  });

  it("reports how many commits the branch is behind its base", () => {
    // Given
    const resolution = aResolutionWithBaseSync({ behindCount: 3, aheadCount: 2 });

    // When
    const view = baseSyncView(resolution);

    // Then
    expect(view).toEqual({ kind: "behind", baseBranch: BASE, behind: 3 });
  });

  it("reports in sync when the branch contains every commit on its base", () => {
    // Given
    const resolution = aResolutionWithBaseSync({ behindCount: 0, aheadCount: 5 });

    // When
    const view = baseSyncView(resolution);

    // Then
    expect(view).toEqual({ kind: "in-sync", baseBranch: BASE });
  });

  it("names the base the daemon compared against rather than the one that was asked for", () => {
    // Given — the counts are meaningless next to a ref they did not come from
    const resolution = aResolutionWithBaseSync({ baseBranch: "origin/release", behindCount: 2 });

    // When
    const view = baseSyncView(resolution);

    // Then
    expect(view).toEqual({ kind: "behind", baseBranch: "origin/release", behind: 2 });
  });
});

describe("canPullFromBase", () => {
  it("offers a pull when the branch is cleanly behind its base", () => {
    // Given / When / Then
    expect(canPullFromBase({ kind: "behind", baseBranch: BASE, behind: 3 })).toBe(true);
  });

  it("offers no pull when the branch is already in sync", () => {
    // Given — a zero-commit merge still runs a git operation and can only surprise
    expect(canPullFromBase({ kind: "in-sync", baseBranch: BASE })).toBe(false);
  });

  it("offers no pull when the branch conflicts with its base", () => {
    expect(canPullFromBase({ kind: "conflicts", baseBranch: BASE, paths: ["a.rs"] })).toBe(false);
  });

  it("offers no pull when the comparison could not be made", () => {
    expect(canPullFromBase({ kind: "unavailable", reason: "not a git repository" })).toBe(false);
  });

  it("offers no pull while the comparison has not arrived", () => {
    expect(canPullFromBase({ kind: "unknown" })).toBe(false);
  });
});
