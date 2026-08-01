import { describe, expect, it } from "bun:test";
import { aStackNode } from "../../../test-utils";
import { buildBranchQueries } from "./branchQueries";

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
    });
  });

  it("compares a root node's branch against the project's default branch", () => {
    // Given — the comparison every stack has, and the one the old poll set never made
    const nodes = [aStackNode({ nodeId: "n1", branch: "feature/auth/token-store" })];

    // When
    const queries = buildBranchQueries(nodes, DEFAULT_BRANCH);

    // Then
    expect(queries).toEqual([
      { branch: "feature/auth/token-store", baseBranch: DEFAULT_BRANCH },
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
    expect(queries).toContainEqual({ branch: "feature/auth/middleware", baseBranch: "" });
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

  it("skips a node that owns no branch", () => {
    // Given — an unspawned node has nothing to resolve
    const nodes = [aStackNode({ nodeId: "n1", branchSuggestion: "feature/auth/token-store" })];

    // When
    const queries = buildBranchQueries(nodes, DEFAULT_BRANCH);

    // Then
    expect(queries).toEqual([]);
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
