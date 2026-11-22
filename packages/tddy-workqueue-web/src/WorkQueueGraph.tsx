/**
 * A work queue's lineage drawn (`#workqueue` 9/14): items as nodes, **line edges** (the branch chain,
 * from PR bases) solid, **dependency edges** (stored behaviour needs) dashed, and endpoints outside the
 * queue (the trunk, an external branch) as their own nodes.
 *
 * Test ids: `work-queue-graph`, `work-queue-graph-node-${id}`, `work-queue-graph-line-${from}-${to}`
 * (`data-kind="line"`), `work-queue-graph-dependency-${from}-${to}` (`data-kind="dependency"`),
 * `work-queue-graph-endpoint-${branch}`.
 */
export type GraphLineEndpoint =
  | { kind: "item"; itemId: string }
  | { kind: "trunk"; branch: string }
  | { kind: "external"; branch: string };

export interface WorkQueueGraphProps {
  items: { itemId: string; title: string }[];
  /** Each item's line parent. */
  lineEdges: { itemId: string; from: GraphLineEndpoint }[];
  /** `to` needs `from`. */
  dependencyEdges: { from: string; to: string }[];
}

export function WorkQueueGraph(_props: WorkQueueGraphProps) {
  // TODO(n9 lineage-graph): implement
  return null;
}
