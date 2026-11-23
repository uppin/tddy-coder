import React from "react";

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

const NODE_W = 170;
const NODE_H = 44;
const COLUMN_GAP = 90;
const ROW_GAP = 28;
const MARGIN = 16;

type Placed = { x: number; y: number; label: string };

const endpointKey = (from: GraphLineEndpoint): string =>
  from.kind === "item" ? `item:${from.itemId}` : `${from.kind}:${from.branch}`;

/** Column of each item: one past its line parent's; an endpoint outside the queue is column 0. */
function columnsOf(props: WorkQueueGraphProps): Map<string, number> {
  const parentOf = new Map<string, string>();
  for (const edge of props.lineEdges) {
    if (edge.from.kind === "item") parentOf.set(edge.itemId, edge.from.itemId);
  }
  const columns = new Map<string, number>();
  const columnOf = (itemId: string, seen: Set<string>): number => {
    const known = columns.get(itemId);
    if (known !== undefined) return known;
    const parent = parentOf.get(itemId);
    // A line parent chain never loops on real data; a loop is cut rather than followed forever.
    const column = parent === undefined || seen.has(parent) ? 1 : columnOf(parent, new Set(seen).add(itemId)) + 1;
    columns.set(itemId, column);
    return column;
  };
  for (const item of props.items) columnOf(item.itemId, new Set());
  return columns;
}

function layoutOf(props: WorkQueueGraphProps) {
  const columns = columnsOf(props);
  const endpoints = new Map<string, Placed>();
  for (const edge of props.lineEdges) {
    if (edge.from.kind !== "item") endpoints.set(endpointKey(edge.from), { x: 0, y: 0, label: edge.from.branch });
  }
  const placed = new Map<string, Placed>();
  const rows: number[] = [];
  const place = (key: string, label: string, column: number) => {
    const row = rows[column] ?? 0;
    rows[column] = row + 1;
    placed.set(key, {
      x: MARGIN + column * (NODE_W + COLUMN_GAP),
      y: MARGIN + row * (NODE_H + ROW_GAP),
      label,
    });
  };
  for (const [key, endpoint] of endpoints) place(key, endpoint.label, 0);
  for (const item of props.items) place(`item:${item.itemId}`, item.title, columns.get(item.itemId) ?? 1);
  const width = MARGIN * 2 + (Math.max(0, rows.length - 1) + 1) * NODE_W + Math.max(0, rows.length - 1) * COLUMN_GAP;
  const height = MARGIN * 2 + Math.max(1, ...rows.map((n) => n ?? 0)) * (NODE_H + ROW_GAP) - ROW_GAP;
  return { placed, width, height };
}

const rightOf = (p: Placed) => ({ x: p.x + NODE_W, y: p.y + NODE_H / 2 });
const leftOf = (p: Placed) => ({ x: p.x, y: p.y + NODE_H / 2 });

export function WorkQueueGraph(props: WorkQueueGraphProps) {
  const { placed, width, height } = layoutOf(props);
  const arrowId = `work-queue-graph-arrow-${React.useId().replace(/:/g, "")}`;

  const edge = (
    kind: "line" | "dependency",
    testId: string,
    from: Placed | undefined,
    to: Placed | undefined,
  ) => {
    if (!from || !to) return null;
    const a = rightOf(from);
    const b = leftOf(to);
    // A dependency bows away from the straight line, so it never hides the line edge between the same pair.
    const bow = kind === "dependency" ? 46 : 0;
    const d = `M ${a.x} ${a.y} C ${a.x + COLUMN_GAP / 2} ${a.y - bow}, ${b.x - COLUMN_GAP / 2} ${b.y - bow}, ${b.x} ${b.y}`;
    return (
      <path
        key={testId}
        data-testid={testId}
        data-kind={kind}
        d={d}
        fill="none"
        stroke={kind === "line" ? "var(--wq-graph-line, #6b7280)" : "var(--wq-graph-dependency, #2563eb)"}
        strokeWidth={kind === "line" ? 2 : 1.5}
        strokeDasharray={kind === "dependency" ? "6 4" : undefined}
        markerEnd={`url(#${arrowId})`}
      />
    );
  };

  const node = (testId: string, p: Placed, external: boolean) => (
    <g key={testId}>
      <rect
        data-testid={testId}
        x={p.x}
        y={p.y}
        width={NODE_W}
        height={NODE_H}
        rx={6}
        fill={external ? "var(--wq-graph-endpoint, #f3f4f6)" : "var(--wq-graph-node, #ffffff)"}
        stroke="var(--wq-graph-line, #6b7280)"
        strokeDasharray={external ? "3 3" : undefined}
      />
      <text x={p.x + 10} y={p.y + NODE_H / 2 + 4} fontSize={13} fill="var(--wq-graph-text, currentColor)">
        {p.label.length > 22 ? `${p.label.slice(0, 21)}…` : p.label}
      </text>
    </g>
  );

  return (
    <div data-testid="work-queue-graph" style={{ overflowX: "auto" }}>
      <svg width={width} height={height} role="img" aria-label="Work queue lineage">
        <defs>
          <marker id={arrowId} viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
            <path d="M 0 0 L 10 5 L 0 10 z" fill="context-stroke" />
          </marker>
        </defs>
        {props.lineEdges.map((e) =>
          edge("line", `work-queue-graph-line-${e.from.kind === "item" ? e.from.itemId : e.from.branch}-${e.itemId}`, placed.get(endpointKey(e.from)), placed.get(`item:${e.itemId}`)),
        )}
        {props.dependencyEdges.map((e) =>
          edge("dependency", `work-queue-graph-dependency-${e.from}-${e.to}`, placed.get(`item:${e.from}`), placed.get(`item:${e.to}`)),
        )}
        {props.items.map((item) => node(`work-queue-graph-node-${item.itemId}`, placed.get(`item:${item.itemId}`)!, false))}
        {[...placed]
          .filter(([key]) => !key.startsWith("item:"))
          .map(([key, p]) => node(`work-queue-graph-endpoint-${key.slice(key.indexOf(":") + 1)}`, p, true))}
      </svg>
    </div>
  );
}
