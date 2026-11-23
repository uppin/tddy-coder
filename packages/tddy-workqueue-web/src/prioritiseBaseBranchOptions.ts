import type { WorkItem } from "./workQueue";

/**
 * The ordered base-branch option list for a planned-PR child session's Start-session dialog.
 *
 * 1. Direct dependency branches: walk `node.parents` in order; for each parent that is NOT merged
 *    (`prStatus?.phase !== "merged"`) and owns a `branch`, include that branch. Order the result by
 *    the dependency's own depth in the stack DAG (longest path from a root, deepest first), ties
 *    broken by the order in `node.parents` (stable). A merged parent contributes nothing; a
 *    branchless parent contributes nothing.
 * 2. Other materialized stack branches: every node with a `branch`, excluding the node itself,
 *    its descendants (basing onto a descendant would create a cycle), merged nodes, and any branch
 *    already listed in step 1 — appended in stack node order.
 *
 * De-duplicate by branch name (first-seen wins). Returns the ordered list of branch names.
 */
export function prioritiseBaseBranchOptions(node: WorkItem, nodes: WorkItem[]): string[] {
  const byId = new Map(nodes.map((n) => [n.itemId, n]));
  const depths = computeDepths(nodes, byId);
  const descendantIds = descendantIdsOf(node.itemId, nodes, byId);

  const directDeps: { branch: string; depth: number; order: number }[] = [];
  node.parents.forEach((parentId, order) => {
    const parent = byId.get(parentId);
    if (!parent || parent.prStatus?.phase === "merged" || !parent.branch) return;
    directDeps.push({
      branch: parent.branch,
      depth: depths.get(parent.itemId) ?? 0,
      order,
    });
  });
  directDeps.sort((a, b) => {
    if (b.depth !== a.depth) return b.depth - a.depth;
    return a.order - b.order;
  });

  const result: string[] = [];
  const seen = new Set<string>();

  for (const { branch } of directDeps) {
    if (seen.has(branch)) continue;
    seen.add(branch);
    result.push(branch);
  }

  for (const workItem of nodes) {
    if (workItem.itemId === node.itemId) continue;
    if (descendantIds.has(workItem.itemId)) continue;
    if (workItem.prStatus?.phase === "merged") continue;
    if (!workItem.branch) continue;
    if (seen.has(workItem.branch)) continue;
    seen.add(workItem.branch);
    result.push(workItem.branch);
  }

  return result;
}

/** Longest path from a root to each node (roots = depth 0). */
function computeDepths(
  nodes: WorkItem[],
  byId: Map<string, WorkItem>,
): Map<string, number> {
  const depths = new Map<string, number>();
  const visiting = new Set<string>();

  const depthOf = (itemId: string): number => {
    if (depths.has(itemId)) return depths.get(itemId)!;
    if (visiting.has(itemId)) return 0;
    visiting.add(itemId);
    const n = byId.get(itemId);
    if (!n || n.parents.length === 0) {
      depths.set(itemId, 0);
      visiting.delete(itemId);
      return 0;
    }
    let maxParentDepth = 0;
    for (const parentId of n.parents) {
      maxParentDepth = Math.max(maxParentDepth, depthOf(parentId));
    }
    const depth = maxParentDepth + 1;
    depths.set(itemId, depth);
    visiting.delete(itemId);
    return depth;
  };

  for (const n of nodes) {
    depthOf(n.itemId);
  }
  return depths;
}

/**
 * Node ids that are descendants of `ancestorId` — nodes that reach `ancestorId` by walking up
 * through `parents`. Cycle-safe via `visited`.
 */
function descendantIdsOf(
  ancestorId: string,
  nodes: WorkItem[],
  byId: Map<string, WorkItem>,
): Set<string> {
  const descendants = new Set<string>();

  for (const candidate of nodes) {
    if (candidate.itemId === ancestorId) continue;
    const visited = new Set<string>();
    const stack: WorkItem[] = [candidate];
    while (stack.length > 0) {
      const current = stack.pop()!;
      if (current.itemId === ancestorId) {
        descendants.add(candidate.itemId);
        break;
      }
      for (const parentId of current.parents) {
        if (visited.has(parentId)) continue;
        visited.add(parentId);
        const parent = byId.get(parentId);
        if (parent) stack.push(parent);
      }
    }
  }

  return descendants;
}
