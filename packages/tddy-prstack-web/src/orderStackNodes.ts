import { topoSortStackNodes, type StackNode } from "./stackPlan";

/**
 * The order the planned-PR list renders its rows in.
 *
 * The reading order is a persisted per-node position (`StackNode.displayOrder`), deliberately kept
 * separate from the dependency graph: a merge, a repoint or a re-parenting rewrites `parents`, and
 * deriving the list's order from that made a row the operator was reading jump position as a
 * consequence of an unrelated event. Rendering strictly by the persisted position is what keeps the
 * list still while the DAG moves — including where the position places a node above its parent.
 *
 * A plan authored before display order existed carries no positions, and falls back to
 * {@link topoSortStackNodes} **wholesale** rather than per node (D25): a half-numbered plan has no
 * coherent total order, and interleaving real positions with invented ones can render a child above
 * its parent — a worse lie than one render of a correct derived order. The next write to the stack
 * numbers every node, after which the fallback stops applying.
 */
export function orderStackNodes(nodes: StackNode[]): StackNode[] {
  const positioned: { node: StackNode; position: number }[] = [];
  for (const node of nodes) {
    // Tested by type rather than against `null`: a node assembled outside `parseStackPlan` carries no
    // `displayOrder` field at all, and `undefined` would slip past a null check and then sort as
    // `undefined - undefined` → `NaN`, leaving the array in raw plan order — the one order this
    // function exists to avoid rendering.
    if (typeof node.displayOrder !== "number") return topoSortStackNodes(nodes);
    positioned.push({ node, position: node.displayOrder });
  }
  // Ties keep the order the plan lists them in — `Array.prototype.sort` is specified as stable.
  return positioned.sort((a, b) => a.position - b.position).map((entry) => entry.node);
}
