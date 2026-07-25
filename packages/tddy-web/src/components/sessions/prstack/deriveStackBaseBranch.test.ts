import { describe, expect, it } from "bun:test";
import { deriveStackBaseBranch } from "./deriveStackBaseBranch";
import type { StackNode } from "./stackPlan";

/**
 * Tests for `deriveStackBaseBranch` — the base branch the Start-Session dialog shows for a planned
 * node. It mirrors the daemon's `effective_base_refs`: a node branches from its nearest non-merged
 * ancestor's branch, collapsing to the stack default when it is a root or all ancestors are merged.
 * Only a created `branch` is a ref — a `branchSuggestion` is a planned name the daemon will not
 * base a spawn onto.
 */

const DEFAULT_BRANCH = "master";

/** A stack node with only the fields these tests care about; everything else gets a valid default. */
function aNode(overrides: Partial<StackNode> & { nodeId: string }): StackNode {
  return {
    nodeId: overrides.nodeId,
    title: overrides.title ?? overrides.nodeId,
    description: "",
    branchSuggestion: null,
    branch: null,
    sessionId: null,
    parents: [],
    prStatus: null,
    childState: null,
    childRecipe: "tdd",
    internalStatus: null,
    ...overrides,
  };
}

describe("deriveStackBaseBranch", () => {
  it("returns the default branch for a root node with no parents", () => {
    // Given
    const n1 = aNode({ nodeId: "n1", branch: "feature/auth/token-store" });

    // When
    const base = deriveStackBaseBranch(n1, [n1], DEFAULT_BRANCH);

    // Then
    expect(base).toBe(DEFAULT_BRANCH);
  });

  it("returns the parent's branch for a node with a single non-merged parent", () => {
    // Given
    const n1 = aNode({ nodeId: "n1", branch: "feature/auth/token-store", prStatus: { phase: "open" } });
    const n2 = aNode({ nodeId: "n2", branch: "feature/auth/middleware", parents: ["n1"] });

    // When
    const base = deriveStackBaseBranch(n2, [n1, n2], DEFAULT_BRANCH);

    // Then
    expect(base).toBe("feature/auth/token-store");
  });

  it("returns the default branch when the only parent has merged", () => {
    // Given
    const n1 = aNode({ nodeId: "n1", branch: "feature/auth/token-store", prStatus: { phase: "merged" } });
    const n2 = aNode({ nodeId: "n2", branch: "feature/auth/middleware", parents: ["n1"] });

    // When
    const base = deriveStackBaseBranch(n2, [n1, n2], DEFAULT_BRANCH);

    // Then
    expect(base).toBe(DEFAULT_BRANCH);
  });

  it("skips a merged parent to the nearest non-merged ancestor's branch", () => {
    // Given — n1 (non-merged) → n2 (merged) → n3
    const n1 = aNode({ nodeId: "n1", branch: "feature/auth/token-store", prStatus: { phase: "open" } });
    const n2 = aNode({
      nodeId: "n2",
      branch: "feature/auth/middleware",
      parents: ["n1"],
      prStatus: { phase: "merged" },
    });
    const n3 = aNode({ nodeId: "n3", branch: "feature/auth/handler", parents: ["n2"] });

    // When
    const base = deriveStackBaseBranch(n3, [n1, n2, n3], DEFAULT_BRANCH);

    // Then
    expect(base).toBe("feature/auth/token-store");
  });

  it("returns the default branch without hanging when parents form a cycle", () => {
    // Given — a malformed stack where n1 and n2 are mutual, merged parents (a cycle)
    const n1 = aNode({
      nodeId: "n1",
      branch: "feature/x/n1",
      parents: ["n2"],
      prStatus: { phase: "merged" },
    });
    const n2 = aNode({
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
    const n1 = aNode({ nodeId: "n1", branchSuggestion: "feature/auth/token-store" });
    const n2 = aNode({ nodeId: "n2", branchSuggestion: "feature/auth/middleware", parents: ["n1"] });

    // When
    const base = deriveStackBaseBranch(n2, [n1, n2], DEFAULT_BRANCH);

    // Then — a suggestion names no ref, so there is nothing to base onto but the default
    expect(base).toBe(DEFAULT_BRANCH);
  });

  it("skips a branchless parent to a predecessor that owns a branch", () => {
    // Given — n2 is planned only; n1 owns a real branch
    const n1 = aNode({ nodeId: "n1", branch: "feature/auth/token-store" });
    const n2 = aNode({ nodeId: "n2", branchSuggestion: "feature/auth/middleware" });
    const n3 = aNode({ nodeId: "n3", parents: ["n2", "n1"] });

    // When
    const base = deriveStackBaseBranch(n3, [n1, n2, n3], DEFAULT_BRANCH);

    // Then
    expect(base).toBe("feature/auth/token-store");
  });
});
