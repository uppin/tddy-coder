import { useEffect, useState } from "react";

/**
 * Logical session table columns (left → right). Single source of truth for header `data-testid`
 * suffixes and responsive visibility policy (GREEN phase).
 */
export type SessionTableColumnKey =
  | "id"
  | "date"
  | "status"
  | "host"
  | "pid"
  | "goal"
  | "workflow"
  | "elapsed"
  | "agent"
  | "model"
  | "actions";

/** Full column order in the Connection screen session tables. */
export const SESSION_TABLE_COLUMN_KEYS_IN_TABLE_ORDER: readonly SessionTableColumnKey[] = [
  "id",
  "date",
  "status",
  "host",
  "pid",
  "goal",
  "workflow",
  "elapsed",
  "agent",
  "model",
  "actions",
];

/**
 * PRD removal order: first listed here is hidden first when width shrinks.
 * Never removed: id, status, actions (not in this list).
 */
export const SESSION_TABLE_COLUMN_REMOVAL_ORDER: readonly SessionTableColumnKey[] = [
  "model",
  "agent",
  "elapsed",
  "workflow",
  "goal",
  "pid",
  "host",
  "date",
];

/** Header label text for each column (stable for thead). */
export const SESSION_TABLE_COLUMN_HEADER_LABEL: Record<SessionTableColumnKey, string> = {
  id: "ID",
  date: "Date",
  status: "Status",
  host: "Host",
  pid: "PID",
  goal: "Goal",
  workflow: "Workflow",
  elapsed: "Elapsed",
  agent: "Agent",
  model: "Model",
  actions: "Actions",
};

/**
 * Minimum viewport width (px) at which each column is still rendered. Keys in
 * {@link SESSION_TABLE_COLUMN_REMOVAL_ORDER} use descending thresholds from model → date so the
 * first column hidden when narrowing is Model, then Agent, …, finally Date.
 * `id`, `status`, and `actions` are always 0 (never removed).
 */
const SESSION_TABLE_COLUMN_MIN_WIDTH_PX: Record<SessionTableColumnKey, number> = {
  id: 0,
  status: 0,
  actions: 0,
  model: 400,
  agent: 380,
  elapsed: 360,
  workflow: 340,
  goal: 320,
  pid: 300,
  host: 280,
  date: 260,
};

/** `data-testid` for a column header, e.g. `session-table-col-header-id`. */
export function sessionTableColumnHeaderTestId(key: SessionTableColumnKey): string {
  return `session-table-col-header-${key}`;
}

/**
 * Viewport widths (px) at which each tier of {@link SESSION_TABLE_COLUMN_REMOVAL_ORDER} applies,
 * sorted ascending — one entry per removable column, aligned with
 * {@link SESSION_TABLE_COLUMN_MIN_WIDTH_PX}.
 */
export function sessionTableRemovalBreakpointsPx(): number[] {
  const tiers = SESSION_TABLE_COLUMN_REMOVAL_ORDER.map((k) => SESSION_TABLE_COLUMN_MIN_WIDTH_PX[k]);
  return [...tiers].sort((a, b) => a - b);
}

/**
 * Column keys rendered at this viewport width, in table order. Progressive hiding follows
 * {@link SESSION_TABLE_COLUMN_REMOVAL_ORDER} via {@link SESSION_TABLE_COLUMN_MIN_WIDTH_PX}.
 */
export function visibleSessionTableColumnKeysForViewportWidth(widthPx: number): SessionTableColumnKey[] {
  return SESSION_TABLE_COLUMN_KEYS_IN_TABLE_ORDER.filter(
    (key) => widthPx >= SESSION_TABLE_COLUMN_MIN_WIDTH_PX[key],
  );
}

/** Subscribe to `window.innerWidth` for responsive column policy (session tables). */
export function useWindowInnerWidthPx(): number {
  const [w, setW] = useState(() =>
    typeof window !== "undefined" ? window.innerWidth : 1024,
  );
  useEffect(() => {
    let raf = 0;
    const onResize = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => setW(window.innerWidth));
    };
    window.addEventListener("resize", onResize);
    return () => {
      cancelAnimationFrame(raf);
      window.removeEventListener("resize", onResize);
    };
  }, []);
  return w;
}
