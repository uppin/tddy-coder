import { describe, expect, it } from "bun:test";
import { aStackNode } from "../../../test-utils";
import {
  branchlessNonMergedParent,
  deriveStackBaseBranch,
  resolveStackBase,
} from "./deriveStackBaseBranch";

/**
 * Tests for `deriveStackBaseBranch` — the base branch the Start-Session dialog shows for a planned
 * node. It mirrors the daemon's `effective_base_refs`: a node branches from its nearest non-merged
 * ancestor's branch, collapsing to the stack default when it is a root or all ancestors are merged.
 * Only a created `branch` is a ref — a `branchSuggestion` is a planned name the daemon will not
 * base a spawn onto.
 *
 * `resolveStackBase` is the discriminated form the same walk produces. The label above cannot tell a
 * root from a chain whose ancestors own no branch — both read as the default branch — yet one is
 * startable and the other is not, so those cases are asserted on the discriminated result.
 */

const DEFAULT_BRANCH = "master";

describe("deriveStackBaseBranch", () => {
  it("returns the default branch for a root node with no parents", () => {
    // Given
    const n1 = aStackNode({ nodeId: "n1", branch: "feature/auth/token-store" });

    // When
    const base = deriveStackBaseBranch(n1, [n1], DEFAULT_BRANCH);

    // Then
    expect(base).toBe(DEFAULT_BRANCH);
  });

  it("returns the parent's branch for a node with a single non-merged parent", () => {
    // Given
    const n1 = aStackNode({ nodeId: "n1", branch: "feature/auth/token-store", prStatus: { phase: "open" } });
    const n2 = aStackNode({ nodeId: "n2", branch: "feature/auth/middleware", parents: ["n1"] });

    // When
    const base = deriveStackBaseBranch(n2, [n1, n2], DEFAULT_BRANCH);

    // Then
    expect(base).toBe("feature/auth/token-store");
  });

  it("returns the default branch when the only parent has merged", () => {
    // Given
    const n1 = aStackNode({ nodeId: "n1", branch: "feature/auth/token-store", prStatus: { phase: "merged" } });
    const n2 = aStackNode({ nodeId: "n2", branch: "feature/auth/middleware", parents: ["n1"] });

    // When
    const base = deriveStackBaseBranch(n2, [n1, n2], DEFAULT_BRANCH);

    // Then
    expect(base).toBe(DEFAULT_BRANCH);
  });

  it("skips a merged parent to the nearest non-merged ancestor's branch", () => {
    // Given — n1 (non-merged) → n2 (merged) → n3
    const n1 = aStackNode({ nodeId: "n1", branch: "feature/auth/token-store", prStatus: { phase: "open" } });
    const n2 = aStackNode({
      nodeId: "n2",
      branch: "feature/auth/middleware",
      parents: ["n1"],
      prStatus: { phase: "merged" },
    });
    const n3 = aStackNode({ nodeId: "n3", branch: "feature/auth/handler", parents: ["n2"] });

    // When
    const base = deriveStackBaseBranch(n3, [n1, n2, n3], DEFAULT_BRANCH);

    // Then
    expect(base).toBe("feature/auth/token-store");
  });

  it("returns the default branch without hanging when parents form a cycle", () => {
    // Given — a malformed stack where n1 and n2 are mutual, merged parents (a cycle)
    const n1 = aStackNode({
      nodeId: "n1",
      branch: "feature/x/n1",
      parents: ["n2"],
      prStatus: { phase: "merged" },
    });
    const n2 = aStackNode({
      nodeId: "n2",
      branch: "feature/x/n2",
      parents: ["n1"],
      prStatus: { phase: "merged" },
    });

    // When
    const base = deriveStackBaseBranch(n1, [n1, n2], DEFAULT_BRANCH);

    // Then — the cycle guard collapses to the default rather than recursing forever
    expect(base).toBe(DEFAULT_BRANCH);
  });

  it("ignores a parent's branch suggestion when its branch is not created yet", () => {
    // Given — the predecessor has only a suggested branch so far
    const n1 = aStackNode({ nodeId: "n1", branchSuggestion: "feature/auth/token-store" });
    const n2 = aStackNode({ nodeId: "n2", branchSuggestion: "feature/auth/middleware", parents: ["n1"] });

    // When
    const base = deriveStackBaseBranch(n2, [n1, n2], DEFAULT_BRANCH);

    // Then — a suggestion names no ref, so there is nothing to base onto but the default
    expect(base).toBe(DEFAULT_BRANCH);
  });

  it("skips a branchless parent to a predecessor that owns a branch", () => {
    // Given — n2 is planned only; n1 owns a real branch
    const n1 = aStackNode({ nodeId: "n1", branch: "feature/auth/token-store" });
    const n2 = aStackNode({ nodeId: "n2", branchSuggestion: "feature/auth/middleware" });
    const n3 = aStackNode({ nodeId: "n3", parents: ["n2", "n1"] });

    // When
    const base = deriveStackBaseBranch(n3, [n1, n2, n3], DEFAULT_BRANCH);

    // Then
    expect(base).toBe("feature/auth/token-store");
  });
});

describe("resolveStackBase", () => {
  it("resolves a root node to the project default branch", () => {
    // Given
    const n1 = aStackNode({ nodeId: "n1", branch: "feature/auth/token-store" });

    // When
    const base = resolveStackBase(n1, [n1]);

    // Then — the default branch exists by construction, so a root is always startable
    expect(base).toEqual({ kind: "default-branch" });
  });

  it("resolves a node whose only ancestor has merged to the project default branch", () => {
    // Given — the predecessor's work is already on the default branch
    const n1 = aStackNode({
      nodeId: "n1",
      branch: "feature/auth/token-store",
      prStatus: { phase: "merged" },
    });
    const n2 = aStackNode({ nodeId: "n2", parents: ["n1"] });

    // When
    const base = resolveStackBase(n2, [n1, n2]);

    // Then — indistinguishable from a root in the flattened label, but equally startable
    expect(base).toEqual({ kind: "default-branch" });
  });

  it("resolves a node to its nearest non-merged ancestor's created branch", () => {
    // Given
    const n1 = aStackNode({
      nodeId: "n1",
      branch: "feature/auth/token-store",
      prStatus: { phase: "open" },
    });
    const n2 = aStackNode({ nodeId: "n2", parents: ["n1"] });

    // When
    const base = resolveStackBase(n2, [n1, n2]);

    // Then
    expect(base).toEqual({ kind: "ancestor-branch", branch: "feature/auth/token-store" });
  });

  it("resolves a node whose ancestor owns no created branch to no-ancestor-branch, naming the planned branch", () => {
    // Given — the predecessor holds only a suggestion, so there is no ref to base onto
    const n1 = aStackNode({ nodeId: "n1", branchSuggestion: "feature/auth/token-store" });
    const n2 = aStackNode({ nodeId: "n2", parents: ["n1"] });

    // When
    const base = resolveStackBase(n2, [n1, n2]);

    // Then — the branch the node is waiting for is the predecessor's planned name
    expect(base).toEqual({
      kind: "no-ancestor-branch",
      plannedBranch: "feature/auth/token-store",
    });
  });

  it("resolves a node whose ancestor has neither a branch nor a suggestion to no-ancestor-branch with no name", () => {
    // Given
    const n1 = aStackNode({ nodeId: "n1" });
    const n2 = aStackNode({ nodeId: "n2", parents: ["n1"] });

    // When
    const base = resolveStackBase(n2, [n1, n2]);

    // Then
    expect(base).toEqual({ kind: "no-ancestor-branch", plannedBranch: null });
  });
});

describe("branchlessNonMergedParent", () => {
  it("names a branchless parent even when a sibling parent owns a branch", () => {
    // Given — n3 depends on n1 (owns a branch) *before* n2 (planned only), so only a check that
    // looks past the first usable parent can find the blocker
    const n1 = aStackNode({ nodeId: "n1", branch: "feature/auth/token-store" });
    const n2 = aStackNode({ nodeId: "n2", branchSuggestion: "feature/auth/middleware" });
    const n3 = aStackNode({ nodeId: "n3", parents: ["n1", "n2"] });

    // When
    const blocking = branchlessNonMergedParent(n3, [n1, n2, n3]);

    // Then — the daemon refuses this spawn on n2, so the sibling's good branch must not mask it
    expect(blocking?.nodeId).toBe("n2");
  });

  it("returns null when every parent owns a branch", () => {
    // Given
    const n1 = aStackNode({ nodeId: "n1", branch: "feature/auth/token-store" });
    const n2 = aStackNode({ nodeId: "n2", branch: "feature/auth/middleware" });
    const n3 = aStackNode({ nodeId: "n3", parents: ["n1", "n2"] });

    // When
    const blocking = branchlessNonMergedParent(n3, [n1, n2, n3]);

    // Then
    expect(blocking).toBeNull();
  });

  it("returns null for a root node with no parents", () => {
    // Given
    const n1 = aStackNode({ nodeId: "n1" });

    // When
    const blocking = branchlessNonMergedParent(n1, [n1]);

    // Then
    expect(blocking).toBeNull();
  });

  it("passes over a merged parent that owns no branch", () => {
    // Given — a merged parent needs no branch: its work is already on the base
    const n1 = aStackNode({ nodeId: "n1", prStatus: { phase: "merged" } });
    const n2 = aStackNode({ nodeId: "n2", parents: ["n1"] });

    // When
    const blocking = branchlessNonMergedParent(n2, [n1, n2]);

    // Then
    expect(blocking).toBeNull();
  });

  it("returns null when a parent id refers to no node in the stack", () => {
    // Given — a malformed plan referencing a parent that is not present
    const n2 = aStackNode({ nodeId: "n2", parents: ["ghost"] });

    // When
    const blocking = branchlessNonMergedParent(n2, [n2]);

    // Then — a dangling reference is a broken plan, not an unmet dependency the row can wait on
    expect(blocking).toBeNull();
  });
});
