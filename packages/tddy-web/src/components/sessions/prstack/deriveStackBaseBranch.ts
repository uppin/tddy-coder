import type { StackNode } from "./stackPlan";

/**
 * The base branch the Start-Session dialog shows for a planned node. Mirrors the daemon's
 * `effective_base_refs`: a node branches from its nearest non-merged ancestor's branch, collapsing
 * to `defaultBranch` when the node is a root or all of its ancestors are merged.
 *
 * Only a parent's concrete `branch` counts. A `branchSuggestion` is a planned name, not a ref: the
 * daemon refuses to base a child onto a parent that owns no branch yet, so previewing the suggestion
 * would promise a base the spawn then rejects. A parent without a branch is passed over exactly like
 * an absent one, collapsing to `defaultBranch` when no ancestor offers a ref. A parent is "merged"
 * when `prStatus.phase === "merged"`.
 */
export function deriveStackBaseBranch(
  node: StackNode,
  nodes: StackNode[],
  defaultBranch: string,
): string {
  const byId = new Map(nodes.map((n) => [n.nodeId, n]));

  // `visited` guards against a malformed stack whose `parents` form a cycle — the same defensive
  // stance `topoSortStackNodes` takes — so a bad `stackPlanJson` can never spin this into infinite
  // recursion on render.
  const nearestNonMergedAncestorBranch = (
    current: StackNode,
    visited: Set<string>,
  ): string | null => {
    for (const parentId of current.parents) {
      const parent = byId.get(parentId);
      if (!parent || visited.has(parent.nodeId)) continue;
      visited.add(parent.nodeId);
      if (parent.prStatus?.phase === "merged") {
        const skipped = nearestNonMergedAncestorBranch(parent, visited);
        if (skipped) return skipped;
        continue;
      }
      if (parent.branch) return parent.branch;
    }
    return null;
  };

  return nearestNonMergedAncestorBranch(node, new Set([node.nodeId])) ?? defaultBranch;
}
