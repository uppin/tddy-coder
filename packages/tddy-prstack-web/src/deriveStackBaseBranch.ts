import type { StackNode } from "./stackPlan";

/**
 * What a planned node's child worktree would be based onto. The three cases are indistinguishable in
 * {@link deriveStackBaseBranch}'s plain string — a root, an all-merged chain and a chain whose
 * ancestors own no branch yet all collapse to the default branch there — yet they differ in whether
 * the node can be started at all, so startability reads this instead.
 */
export type StackBase =
  /**
   * The project's default branch: the node is a root, or every ancestor has merged. Startable — the
   * default branch exists by construction.
   */
  | { kind: "default-branch" }
  /** The nearest non-merged ancestor's created branch. Startable once `origin/<branch>` exists. */
  | { kind: "ancestor-branch"; branch: string }
  /**
   * A non-merged ancestor owns no created branch, so there is no ref to base onto — the daemon
   * refuses such a spawn (`Stack::base_ref_for_spawn`). `plannedBranch` is that ancestor's suggested
   * name when it has one, so the blocked row can say which branch it is waiting for.
   */
  | { kind: "no-ancestor-branch"; plannedBranch: string | null };

/**
 * Resolve what a planned node's child worktree would be based onto. Mirrors the daemon's
 * `effective_base_refs`: a node branches from its nearest non-merged ancestor's branch, collapsing to
 * the project default when the node is a root or all of its ancestors are merged.
 *
 * Only a parent's concrete `branch` counts. A `branchSuggestion` is a planned name, not a ref: the
 * daemon refuses to base a child onto a parent that owns no branch yet.
 */
export function resolveStackBase(node: StackNode, nodes: StackNode[]): StackBase {
  const byId = new Map(nodes.map((n) => [n.nodeId, n]));

  // `visited` guards against a malformed stack whose `parents` form a cycle — the same defensive
  // stance `topoSortStackNodes` takes — so a bad `stackPlanJson` can never spin this into infinite
  // recursion on render.
  const resolve = (current: StackNode, visited: Set<string>): StackBase => {
    // The first non-merged ancestor found to own no branch. Held rather than returned immediately so
    // a sibling parent that does own a branch still wins (the base is the nearest available ref).
    let branchless: StackBase | null = null;

    for (const parentId of current.parents) {
      const parent = byId.get(parentId);
      if (!parent || visited.has(parent.nodeId)) continue;
      visited.add(parent.nodeId);
      if (parent.prStatus?.phase === "merged") {
        const skipped = resolve(parent, visited);
        if (skipped.kind === "ancestor-branch") return skipped;
        // A merged parent whose own ancestors are blocked passes that block on; its `origin` ref may
        // already be gone, so it is no base either.
        if (skipped.kind === "no-ancestor-branch") branchless ??= skipped;
        continue;
      }
      if (parent.branch) return { kind: "ancestor-branch", branch: parent.branch };
      branchless ??= { kind: "no-ancestor-branch", plannedBranch: parent.branchSuggestion };
    }

    // No parents at all (a root), or every one of them merged onto the default branch.
    return branchless ?? { kind: "default-branch" };
  };

  return resolve(node, new Set([node.nodeId]));
}

/**
 * The node's first direct parent that has neither merged nor created a branch, or `null` when every
 * direct parent is a usable base.
 *
 * This mirrors the daemon's spawn gate (`Stack::base_ref_for_spawn`) exactly: a flat check over the
 * node's *direct* parents that refuses when **any** of them is non-merged and branchless — even when a
 * sibling parent owns a perfectly good branch.
 *
 * {@link resolveStackBase} deliberately does not model that. It answers "what would the base be?" the
 * way `effective_base_refs` does, walking past a branchless parent to a sibling that owns a ref. Both
 * are needed, and they are not the same question: the base names the dialog's label, this decides
 * startability. Without this check a multi-parent node would be offered a Start-session button the
 * daemon then rejects with `ChangesetInvalid`.
 */
export function branchlessNonMergedParent(node: StackNode, nodes: StackNode[]): StackNode | null {
  const byId = new Map(nodes.map((n) => [n.nodeId, n]));
  for (const parentId of node.parents) {
    const parent = byId.get(parentId);
    // An unknown parent id is not a blocker: the daemon's gate likewise only refuses on a parent it
    // can resolve, and a dangling reference is a malformed plan rather than an unmet dependency.
    if (!parent) continue;
    if (parent.prStatus?.phase === "merged") continue;
    if (!parent.branch) return parent;
  }
  return null;
}

/**
 * The base branch the Start-Session dialog shows for a planned node — {@link resolveStackBase}
 * flattened to a name, with every non-`ancestor-branch` outcome reading as `defaultBranch`.
 *
 * This is the dialog's label only. It cannot express *why* an ancestor named no branch, so anything
 * deciding whether a node is startable must use {@link resolveStackBase} directly.
 */
export function deriveStackBaseBranch(
  node: StackNode,
  nodes: StackNode[],
  defaultBranch: string,
): string {
  const base = resolveStackBase(node, nodes);
  return base.kind === "ancestor-branch" ? base.branch : defaultBranch;
}
