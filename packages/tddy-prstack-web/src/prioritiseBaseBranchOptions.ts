import type { StackNode } from "./stackPlan";

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
export function prioritiseBaseBranchOptions(node: StackNode, nodes: StackNode[]): string[] {
  const byId = new Map(nodes.map((n) => [n.nodeId, n]));
  const depths = computeDepths(nodes, byId);
  const descendantIds = descendantIdsOf(node.nodeId, nodes, byId);

  const directDeps: { branch: string; depth: number; order: number }[] = [];
  node.parents.forEach((parentId, order) => {
    const parent = byId.get(parentId);
    if (!parent || parent.prStatus?.phase === "merged" || !parent.branch) return;
    directDeps.push({
      branch: parent.branch,
      depth: depths.get(parent.nodeId) ?? 0,
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

  for (const stackNode of nodes) {
    if (stackNode.nodeId === node.nodeId) continue;
    if (descendantIds.has(stackNode.nodeId)) continue;
    if (stackNode.prStatus?.phase === "merged") continue;
    if (!stackNode.branch) continue;
    if (seen.has(stackNode.branch)) continue;
    seen.add(stackNode.branch);
    result.push(stackNode.branch);
  }

  return result;
}

/** Longest path from a root to each node (roots = depth 0). */
function computeDepths(
  nodes: StackNode[],
  byId: Map<string, StackNode>,
): Map<string, number> {
  const depths = new Map<string, number>();
  const visiting = new Set<string>();

  const depthOf = (nodeId: string): number => {
    if (depths.has(nodeId)) return depths.get(nodeId)!;
    if (visiting.has(nodeId)) return 0;
    visiting.add(nodeId);
    const n = byId.get(nodeId);
    if (!n || n.parents.length === 0) {
      depths.set(nodeId, 0);
      visiting.delete(nodeId);
      return 0;
    }
    let maxParentDepth = 0;
    for (const parentId of n.parents) {
      maxParentDepth = Math.max(maxParentDepth, depthOf(parentId));
    }
    const depth = maxParentDepth + 1;
    depths.set(nodeId, depth);
    visiting.delete(nodeId);
    return depth;
  };

  for (const n of nodes) {
    depthOf(n.nodeId);
  }
  return depths;
}

/**
 * Node ids that are descendants of `ancestorId` — nodes that reach `ancestorId` by walking up
 * through `parents`. Cycle-safe via `visited`.
 */
function descendantIdsOf(
  ancestorId: string,
  nodes: StackNode[],
  byId: Map<string, StackNode>,
): Set<string> {
  const descendants = new Set<string>();

  for (const candidate of nodes) {
    if (candidate.nodeId === ancestorId) continue;
    const visited = new Set<string>();
    const stack: StackNode[] = [candidate];
    while (stack.length > 0) {
      const current = stack.pop()!;
      if (current.nodeId === ancestorId) {
        descendants.add(candidate.nodeId);
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
