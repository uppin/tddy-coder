import React, { useLayoutEffect, useState } from "react";

/** Width of the right gutter the brackets are drawn in; the list reserves it as padding. */
export const CONNECTOR_GUTTER = 36;

export interface ConnectorEdge {
  kind: "line" | "dependency";
  /** The parent task (a line edge's base, a dependency's needed task). */
  from: string;
  /** The child task. */
  to: string;
}

interface Box {
  top: number;
  height: number;
  right: number;
}

/** `id` as the inside of a double-quoted attribute selector. */
export const quoted = (id: string) => id.replace(/["\\]/g, "\\$&");

const connectorTestId = (edge: ConnectorEdge) =>
  edge.kind === "line"
    ? `work-item-connector-${edge.from}-${edge.to}`
    : `work-item-dependency-connector-${edge.from}-${edge.to}`;

/** Lane (0, 1, …) per edge: edges whose vertical spans overlap take different lanes, so brackets never coincide. */
function lanesOf(edges: ConnectorEdge[], boxes: Record<string, Box>): number[] {
  const spans = edges.map((e) => {
    const a = boxes[e.from];
    const b = boxes[e.to];
    if (!a || !b) return null;
    const ya = a.top + a.height / 2;
    const yb = b.top + b.height / 2;
    return [Math.min(ya, yb), Math.max(ya, yb)] as const;
  });
  const lanes: number[] = [];
  spans.forEach((span, i) => {
    if (!span) return void lanes.push(0);
    const taken = new Set<number>();
    for (let j = 0; j < i; j += 1) {
      const other = spans[j];
      if (other && other[0] < span[1] && span[0] < other[1]) taken.add(lanes[j]);
    }
    let lane = 0;
    while (taken.has(lane)) lane += 1;
    lanes.push(lane);
  });
  return lanes;
}

/**
 * Brackets on the right gutter of the task list joining the right sides of linked tasks: solid for a
 * PR-stack line edge (a child's PR base is the parent's branch), dashed for a stored dependency.
 * Positions are read off the rendered rows (`work-item-row-<id>`) inside `container`, so the
 * brackets follow the boxes however tall a row grows.
 */
export function WorkItemConnectors({
  container,
  edges,
  layoutKey,
}: {
  container: HTMLElement | null;
  edges: ConnectorEdge[];
  /** Changes whenever the rows may have moved. */
  layoutKey: string;
}) {
  const [boxes, setBoxes] = useState<Record<string, Box>>({});

  useLayoutEffect(() => {
    if (!container) return;
    const measure = () => {
      const next: Record<string, Box> = {};
      for (const edge of edges) {
        for (const id of [edge.from, edge.to]) {
          const rowEl = container.querySelector<HTMLElement>(`[data-testid="work-item-row-${quoted(id)}"]`);
          if (rowEl) {
            next[id] = { top: rowEl.offsetTop, height: rowEl.offsetHeight, right: rowEl.offsetLeft + rowEl.offsetWidth };
          }
        }
      }
      setBoxes((prev) => (JSON.stringify(prev) === JSON.stringify(next) ? prev : next));
    };
    measure();
    // A document without layout observation measures once; every browser has it.
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(measure);
    observer.observe(container);
    container.querySelectorAll("[data-testid^='work-item-row-']").forEach((el) => observer.observe(el));
    return () => observer.disconnect();
  }, [container, edges, layoutKey]);

  const lanes = lanesOf(edges, boxes);
  return (
    <svg
      aria-hidden="true"
      data-testid="work-item-connectors"
      style={{ position: "absolute", left: 0, top: 0, width: "100%", height: "100%", pointerEvents: "none", overflow: "visible" }}
    >
      {edges.map((edge, i) => {
        const a = boxes[edge.from];
        const b = boxes[edge.to];
        if (!a || !b) return null;
        const x = a.right;
        const y1 = a.top + a.height / 2;
        const y2 = b.top + b.height / 2;
        const xg = Math.max(a.right, b.right) + 10 + lanes[i] * 8;
        const r = Math.min(6, Math.abs(y2 - y1) / 2);
        const dir = y2 >= y1 ? 1 : -1;
        const d = `M ${x} ${y1} H ${xg - r} Q ${xg} ${y1} ${xg} ${y1 + dir * r} V ${y2 - dir * r} Q ${xg} ${y2} ${xg - r} ${y2} H ${b.right}`;
        return (
          <path
            key={connectorTestId(edge)}
            data-testid={connectorTestId(edge)}
            data-kind={edge.kind}
            d={d}
            fill="none"
            stroke={edge.kind === "line" ? "var(--wq-graph-line, #6b7280)" : "var(--wq-graph-dependency, #2563eb)"}
            strokeWidth={edge.kind === "line" ? 2 : 1.5}
            strokeDasharray={edge.kind === "dependency" ? "6 4" : undefined}
          />
        );
      })}
    </svg>
  );
}
