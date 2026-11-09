import type { StackNode } from "./stackPlan";

/**
 * The titles of the planned PRs `node` is stacked on, in the order it lists them — the first parent
 * is the primary spine, so the order carries meaning.
 *
 * `StackNode.parents` records node ids ("n1", "n4"), which say nothing to an operator reading the
 * panel. A parent id no node in the plan carries is **skipped** rather than rendered: a plan
 * referencing a node that does not exist is malformed, not an unmet dependency — the same stance
 * `deriveStackBaseBranch` and `startBlockers` already take.
 */
export function parentTitles(node: StackNode, nodes: StackNode[]): string[] {
  const titleByNodeId = new Map(nodes.map((n) => [n.nodeId, n.title]));
  return node.parents
    .map((parentId) => titleByNodeId.get(parentId))
    .filter((title): title is string => title !== undefined);
}
