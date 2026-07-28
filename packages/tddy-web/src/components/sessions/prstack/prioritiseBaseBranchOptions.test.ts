import { describe, expect, it } from "bun:test";
import { prioritiseBaseBranchOptions } from "./prioritiseBaseBranchOptions";
import type { StackNode } from "./stackPlan";

/**
 * Tests for `prioritiseBaseBranchOptions` — the ordered base-branch option list the Start-Session
 * dialog's "Base branch" `<select>` renders for a planned-PR child session.
 *
 * The list is: the node's direct dependency branches first (non-merged parents that own a branch,
 * ordered by the dependency's own depth in the stack DAG — longest path from a root, deepest first —
 * ties broken by the order in `node.parents`), then every other materialized stack branch that is
 * neither the node itself nor one of its descendants, in stack node order. Merged and branchless
 * nodes contribute nothing.
 */

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

describe("prioritiseBaseBranchOptions", () => {
  it("returns an empty list for a root node with no other materialized branches", () => {
    // Given
    const n1 = aNode({ nodeId: "n1", branchSuggestion: "feature/stack/n1" });

    // When
    const options = prioritiseBaseBranchOptions(n1, [n1]);

    // Then
    expect(options).toEqual([]);
  });

  it("lists the single direct dependency branch for a linear chain", () => {
    // Given
    const n1 = aNode({ nodeId: "n1", branch: "feature/stack/n1", prStatus: { phase: "open" } });
    const n2 = aNode({ nodeId: "n2", branchSuggestion: "feature/stack/n2", parents: ["n1"] });

    // When
    const options = prioritiseBaseBranchOptions(n2, [n1, n2]);

    // Then
    expect(options).toEqual(["feature/stack/n1"]);
  });

  it("lists both direct dependency branches in node.parents order when they are at the same depth", () => {
    // Given — the attach-start diamond: parents [attach-proto, attach-store], both roots (depth 0).
    const proto = aNode({
      nodeId: "attach-proto",
      branch: "feature/session-attach-docs/attach-proto",
      prStatus: { phase: "open" },
    });
    const store = aNode({
      nodeId: "attach-store",
      branch: "feature/session-attach-docs/attach-store",
      prStatus: { phase: "open" },
    });
    const start = aNode({
      nodeId: "attach-start",
      branchSuggestion: "feature/session-attach-docs/attach-start",
      parents: ["attach-proto", "attach-store"],
    });

    // When
    const options = prioritiseBaseBranchOptions(start, [proto, store, start]);

    // Then — attach-proto (first in node.parents) precedes attach-store.
    expect(options).toEqual([
      "feature/session-attach-docs/attach-proto",
      "feature/session-attach-docs/attach-store",
    ]);
  });

  it("orders direct dependency branches by the dependency's own depth (deepest first) for a diamond", () => {
    // Given — n3 depends on [n1, n2] and n2 depends on n1: n2 is depth 1, n1 is depth 0. n2 is listed
    // second in node.parents but is deeper, so it must come first.
    const n1 = aNode({ nodeId: "n1", branch: "feature/stack/n1", prStatus: { phase: "open" } });
    const n2 = aNode({
      nodeId: "n2",
      branch: "feature/stack/n2",
      prStatus: { phase: "open" },
      parents: ["n1"],
    });
    const n3 = aNode({
      nodeId: "n3",
      branchSuggestion: "feature/stack/n3",
      parents: ["n1", "n2"],
    });

    // When
    const options = prioritiseBaseBranchOptions(n3, [n1, n2, n3]);

    // Then — n2 (depth 1) precedes n1 (depth 0).
    expect(options).toEqual(["feature/stack/n2", "feature/stack/n1"]);
  });

  it("appends other materialized stack branches after the direct dependencies", () => {
    // Given — n3 depends on n1 only; n2 is a materialized sibling root, not a direct dependency.
    const n1 = aNode({ nodeId: "n1", branch: "feature/stack/n1", prStatus: { phase: "open" } });
    const n2 = aNode({ nodeId: "n2", branch: "feature/stack/n2", prStatus: { phase: "open" } });
    const n3 = aNode({ nodeId: "n3", branchSuggestion: "feature/stack/n3", parents: ["n1"] });

    // When
    const options = prioritiseBaseBranchOptions(n3, [n1, n2, n3]);

    // Then — n1 (direct dep) first, then n2 (other materialized branch).
    expect(options).toEqual(["feature/stack/n1", "feature/stack/n2"]);
  });

  it("excludes the node's own descendants from the other-branches section to avoid a cycle", () => {
    // Given — n1 is materialized; n2 depends on n1; n3 depends on n2. Starting n2's session, the
    // "other" section must not offer n3 (a descendant of n2) as a base.
    const n1 = aNode({ nodeId: "n1", branch: "feature/stack/n1", prStatus: { phase: "open" } });
    const n2 = aNode({
      nodeId: "n2",
      branch: "feature/stack/n2",
      prStatus: { phase: "open" },
      parents: ["n1"],
    });
    const n3 = aNode({ nodeId: "n3", branchSuggestion: "feature/stack/n3", parents: ["n2"] });

    // When — options for n2: direct dep n1; n3 is a descendant and must be excluded.
    const options = prioritiseBaseBranchOptions(n2, [n1, n2, n3]);

    // Then — only n1 (n3 is a descendant of n2; n2 itself is the node).
    expect(options).toEqual(["feature/stack/n1"]);
  });

  it("skips merged parents and excludes merged branches from the other-branches section", () => {
    // Given — n1 (merged) and n2 (open) are both roots; n3 depends on both. A merged parent contributes
    // no ref (its origin ref may be gone), and a merged non-direct branch is not offered either.
    const n1 = aNode({
      nodeId: "n1",
      branch: "feature/stack/n1",
      prStatus: { phase: "merged" },
    });
    const n2 = aNode({ nodeId: "n2", branch: "feature/stack/n2", prStatus: { phase: "open" } });
    const n3 = aNode({
      nodeId: "n3",
      branchSuggestion: "feature/stack/n3",
      parents: ["n1", "n2"],
    });

    // When
    const options = prioritiseBaseBranchOptions(n3, [n1, n2, n3]);

    // Then — only n2 (n1 is merged; no other materialized branches remain).
    expect(options).toEqual(["feature/stack/n2"]);
  });

  it("skips a branchless direct parent (no ref to offer)", () => {
    // Given — n1 is a planned, branchless parent of n2; n2 also has a materialized parent n3.
    const n1 = aNode({ nodeId: "n1", branchSuggestion: "feature/stack/n1" });
    const n3 = aNode({ nodeId: "n3", branch: "feature/stack/n3", prStatus: { phase: "open" } });
    const n2 = aNode({
      nodeId: "n2",
      branchSuggestion: "feature/stack/n2",
      parents: ["n1", "n3"],
    });

    // When
    const options = prioritiseBaseBranchOptions(n2, [n1, n3, n2]);

    // Then — only n3 (n1 owns no branch; n3 is the only usable direct dep).
    expect(options).toEqual(["feature/stack/n3"]);
  });

  it("de-duplicates a branch that is both a direct dependency and appears elsewhere in the stack", () => {
    // Given — n1 and n2 both own the same branch name (a malformed but defensive case); n3 depends on
    // both. The branch must appear exactly once.
    const n1 = aNode({ nodeId: "n1", branch: "feature/stack/shared", prStatus: { phase: "open" } });
    const n2 = aNode({ nodeId: "n2", branch: "feature/stack/shared", prStatus: { phase: "open" } });
    const n3 = aNode({
      nodeId: "n3",
      branchSuggestion: "feature/stack/n3",
      parents: ["n1", "n2"],
    });

    // When
    const options = prioritiseBaseBranchOptions(n3, [n1, n2, n3]);

    // Then — the shared branch appears once (first-seen wins).
    expect(options).toEqual(["feature/stack/shared"]);
  });
});
