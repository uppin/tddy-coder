import { describe, expect, it } from "bun:test";
import { aStackNode } from "../../../test-utils";
import { buildBranchQueries, plannedNameBranches } from "./branchQueries";

/**
 * Tests for `buildBranchQueries` — what the PR-Stack screen asks `QueryBranch` to resolve.
 *
 * A branch's standing against its base is a comparison, so the poll has to name both refs. The set
 * used to be branch names alone, built from the branches nodes own plus each node's *ancestor* base
 * — which meant a **root** node, whose base is the project's default branch, had no comparison in
 * the set at all. That is the one comparison every stack has.
 *
 * Every base in the set is by definition some node's own branch, so pairing the two adds no queries
 * of its own: the poll stays one call per branch, which matters because each one reaches GitHub for
 * that branch's PR status.
 *
 * A node that owns **no** branch is additionally queried on its `branch_suggestion`, whose answer is
 * read for its `pr` leg alone — the one leg that survives the host boundary, because it asks the
 * GitHub API by head branch rather than this daemon's disk. Such a query is tagged `planned-name`
 * and names no base: a suggestion is not a ref (D1), so there is nothing to compare and nothing that
 * may feed base resolution or the spawn gate.
 *
 * PRD: docs/ft/coder/pr-stack-live-status.md § Cross-host planned PRs (D41).
 */

const DEFAULT_BRANCH = "origin/master";

describe("buildBranchQueries", () => {
  it("compares a stacked node's branch against its predecessor's branch", () => {
    // Given
    const nodes = [
      aStackNode({ nodeId: "n1", branch: "feature/auth/token-store" }),
      aStackNode({ nodeId: "n2", branch: "feature/auth/middleware", parents: ["n1"] }),
    ];

    // When
    const queries = buildBranchQueries(nodes, DEFAULT_BRANCH);

    // Then
    expect(queries).toContainEqual({
      branch: "feature/auth/middleware",
      baseBranch: "feature/auth/token-store",
      kind: "owned-branch",
    });
  });

  it("compares a root node's branch against the project's default branch", () => {
    // Given — the comparison every stack has, and the one the old poll set never made
    const nodes = [aStackNode({ nodeId: "n1", branch: "feature/auth/token-store" })];

    // When
    const queries = buildBranchQueries(nodes, DEFAULT_BRANCH);

    // Then
    expect(queries).toEqual([
      { branch: "feature/auth/token-store", baseBranch: DEFAULT_BRANCH, kind: "owned-branch" },
    ]);
  });

  it("compares a node past a merged predecessor against the default branch", () => {
    // Given — a merged ancestor's ref may be gone, so the base collapses past it
    const nodes = [
      aStackNode({
        nodeId: "n1",
        branch: "feature/auth/token-store",
        prStatus: { phase: "merged" },
      }),
      aStackNode({ nodeId: "n2", branch: "feature/auth/middleware", parents: ["n1"] }),
    ];

    // When
    const queries = buildBranchQueries(nodes, DEFAULT_BRANCH);

    // Then
    expect(queries).toContainEqual({
      branch: "feature/auth/middleware",
      baseBranch: DEFAULT_BRANCH,
      kind: "owned-branch",
    });
  });

  it("names no base for a node whose ancestors own no branch", () => {
    // Given — an unnamed base is reported unavailable rather than compared against a guess
    const nodes = [
      aStackNode({ nodeId: "n1", branchSuggestion: "feature/auth/token-store" }),
      aStackNode({ nodeId: "n2", branch: "feature/auth/middleware", parents: ["n1"] }),
    ];

    // When
    const queries = buildBranchQueries(nodes, DEFAULT_BRANCH);

    // Then
    expect(queries).toContainEqual({
      branch: "feature/auth/middleware",
      baseBranch: "",
      kind: "owned-branch",
    });
  });

  it("queries a branch once when it is both a node's branch and another node's base", () => {
    // Given
    const nodes = [
      aStackNode({ nodeId: "n1", branch: "feature/auth/token-store" }),
      aStackNode({ nodeId: "n2", branch: "feature/auth/middleware", parents: ["n1"] }),
      aStackNode({ nodeId: "n3", branch: "feature/auth/login", parents: ["n1"] }),
    ];

    // When
    const queries = buildBranchQueries(nodes, DEFAULT_BRANCH);

    // Then — one call per branch: each one reaches GitHub for that branch's PR status
    expect(queries.map((q) => q.branch)).toEqual([
      "feature/auth/login",
      "feature/auth/middleware",
      "feature/auth/token-store",
    ]);
  });

  it("queries a branchless node's planned name, with no base and tagged as planned", () => {
    // Given — an unspawned node: the only thing that can be resolved for it is its PR
    const nodes = [aStackNode({ nodeId: "n1", branchSuggestion: "feature/auth/token-store" })];

    // When
    const queries = buildBranchQueries(nodes, DEFAULT_BRANCH);

    // Then — no base: a suggestion is not a ref, so there is no comparison to make (D29, D41)
    expect(queries).toEqual([
      { branch: "feature/auth/token-store", baseBranch: "", kind: "planned-name" },
    ]);
  });

  it("skips a node that owns neither a branch nor a planned name", () => {
    // Given — a node the planner gave no branch name at all
    const nodes = [aStackNode({ nodeId: "n1" })];

    // When
    const queries = buildBranchQueries(nodes, DEFAULT_BRANCH);

    // Then
    expect(queries).toEqual([]);
  });

  it("queries a planned name once when two branchless nodes share it", () => {
    // Given — a malformed plan; the poll must not double its GitHub request rate over it
    const nodes = [
      aStackNode({ nodeId: "n1", branchSuggestion: "feature/auth/token-store" }),
      aStackNode({ nodeId: "n2", branchSuggestion: "feature/auth/token-store" }),
    ];

    // When
    const queries = buildBranchQueries(nodes, DEFAULT_BRANCH);

    // Then
    expect(queries).toEqual([
      { branch: "feature/auth/token-store", baseBranch: "", kind: "planned-name" },
    ]);
  });

  it("keeps the owned query when another node only plans the same branch name", () => {
    // Given — n1 created the branch n2 is still only planning to create
    const nodes = [
      aStackNode({ nodeId: "n1", branch: "feature/auth/token-store" }),
      aStackNode({ nodeId: "n2", branchSuggestion: "feature/auth/token-store" }),
    ];

    // When
    const queries = buildBranchQueries(nodes, DEFAULT_BRANCH);

    // Then — a ref that exists answers more than a name, so it must not be downgraded
    expect(queries).toEqual([
      { branch: "feature/auth/token-store", baseBranch: DEFAULT_BRANCH, kind: "owned-branch" },
    ]);
  });

  it("keeps the owned query when the node that only plans the name is listed first", () => {
    // Given — the same collision with the plan listing the planner before the owner, which is the
    // order a single pass over the nodes would resolve the wrong way round
    const nodes = [
      aStackNode({ nodeId: "n2", branchSuggestion: "feature/auth/token-store" }),
      aStackNode({ nodeId: "n1", branch: "feature/auth/token-store" }),
    ];

    // When
    const queries = buildBranchQueries(nodes, DEFAULT_BRANCH);

    // Then — which kind the poll set carries must be a fact about the stack, not about plan order
    expect(queries).toEqual([
      { branch: "feature/auth/token-store", baseBranch: DEFAULT_BRANCH, kind: "owned-branch" },
    ]);
  });

  it("returns the queries in branch order whatever order the plan lists nodes in", () => {
    // Given — a stable key, so the poll does not re-subscribe on an unrelated re-render
    const nodes = [
      aStackNode({ nodeId: "n2", branch: "feature/auth/middleware" }),
      aStackNode({ nodeId: "n1", branch: "feature/auth/token-store" }),
    ];

    // When
    const queries = buildBranchQueries(nodes, DEFAULT_BRANCH);

    // Then
    expect(queries.map((q) => q.branch)).toEqual([
      "feature/auth/middleware",
      "feature/auth/token-store",
    ]);
  });
});

/**
 * `plannedNameBranches` — the names a row may read a planned PR from.
 *
 * The set is taken from the queries rather than re-derived from the nodes, because the two disagree
 * in exactly the case that matters: a branchless node whose suggestion collides with a branch another
 * node created still looks like a planned-name node, while the answer under that name describes the
 * other node's ref.
 */
describe("plannedNameBranches", () => {
  it("names the branch a branchless node is polled on", () => {
    // Given
    const nodes = [aStackNode({ nodeId: "n1", branchSuggestion: "feature/auth/token-store" })];

    // When
    const planned = plannedNameBranches(buildBranchQueries(nodes, DEFAULT_BRANCH));

    // Then
    expect([...planned]).toEqual(["feature/auth/token-store"]);
  });

  it("names no branch a node actually owns", () => {
    // Given
    const nodes = [aStackNode({ nodeId: "n1", branch: "feature/auth/token-store" })];

    // When
    const planned = plannedNameBranches(buildBranchQueries(nodes, DEFAULT_BRANCH));

    // Then
    expect([...planned]).toEqual([]);
  });

  it("names no branch one node owns and another only plans", () => {
    // Given — n1 created the branch n2 is still planning, so the one query for that name is owned
    const nodes = [
      aStackNode({ nodeId: "n1", branch: "feature/auth/token-store" }),
      aStackNode({ nodeId: "n2", branchSuggestion: "feature/auth/token-store" }),
    ];

    // When
    const planned = plannedNameBranches(buildBranchQueries(nodes, DEFAULT_BRANCH));

    // Then — n2's row must not present n1's live PR as its own
    expect([...planned]).toEqual([]);
  });
});
