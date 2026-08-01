import { resolveStackBase } from "./deriveStackBaseBranch";
import type { StackNode } from "./stackPlan";

/** One `QueryBranch` call: a branch, and the base its standing is measured against. */
export interface BranchQuery {
  branch: string;
  /**
   * The base the daemon compares `branch` against. Empty when no ancestor owns a ref — an unnamed
   * base is reported back as unavailable rather than substituted with a guess (D29), since the
   * number beside a row must describe the same base the row's own base line shows.
   */
  baseBranch: string;
}

/**
 * What the PR-Stack screen asks `QueryBranch` to resolve: one call per branch a node owns, each
 * paired with that node's base.
 *
 * The set used to be branch names alone — the branches nodes own plus each node's *ancestor* base —
 * which left a **root** node, whose base is the project's default branch, with no comparison at all.
 * That is the one comparison every stack has.
 *
 * Pairing adds no calls of its own: every ancestor base is by definition some node's own branch, so
 * it is already in the set. That matters because each call reaches GitHub for that branch's PR
 * status, and polling more branches than the stack owns would spend the hourly rate limit on
 * comparisons nobody reads.
 *
 * Sorted by branch so the result is a stable poll key: an unrelated re-render that hands over an
 * equivalent set must not re-subscribe the poll.
 */
export function buildBranchQueries(nodes: StackNode[], defaultBranch: string): BranchQuery[] {
  const byBranch = new Map<string, BranchQuery>();

  for (const node of nodes) {
    // An unspawned node owns no branch, so it has nothing to resolve.
    if (!node.branch) continue;
    const base = resolveStackBase(node, nodes);
    const baseBranch =
      base.kind === "ancestor-branch"
        ? base.branch
        : base.kind === "default-branch"
          ? defaultBranch
          : "";
    const existing = byBranch.get(node.branch);
    // Two nodes claiming one branch is a malformed plan, not a state to resolve twice. Prefer the
    // entry that names a base: a comparison against a base beats no comparison at all.
    if (existing && existing.baseBranch) continue;
    byBranch.set(node.branch, { branch: node.branch, baseBranch });
  }

  return [...byBranch.values()].sort((a, b) => a.branch.localeCompare(b.branch));
}
