/**
 * Logical session table columns (left → right). Single source of truth for header `data-testid`
 * suffixes and responsive visibility policy.
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

/** Full wide table header `data-testid` sequence (left → right). */
export const SESSION_TABLE_HEADER_TESTIDS_IN_TABLE_ORDER: readonly string[] =
  SESSION_TABLE_COLUMN_KEYS_IN_TABLE_ORDER.map(sessionTableColumnHeaderTestId);

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

/** Header `data-testid` values visible at this viewport width, in table order. */
export function visibleSessionTableHeaderTestIdsForWidth(widthPx: number): readonly string[] {
  return visibleSessionTableColumnKeysForViewportWidth(widthPx).map(sessionTableColumnHeaderTestId);
}

/**
 * Effective width (px) for session table column policy: the narrower of the browser window and the
 * session-tables host element when the host is measured; otherwise the window width.
 */
export function effectiveSessionTableLayoutWidthPx(
  windowInnerWidthPx: number,
  sessionTableHostWidthPx: number | null,
): number {
  if (sessionTableHostWidthPx == null || sessionTableHostWidthPx <= 0) {
    return windowInnerWidthPx;
  }
  return Math.min(windowInnerWidthPx, sessionTableHostWidthPx);
}

/** Column keys when both window and session-table host widths are known (e.g. split layouts). */
export function visibleSessionTableColumnKeysForLayout(
  windowInnerWidthPx: number,
  sessionTableHostWidthPx: number | null,
): SessionTableColumnKey[] {
  return visibleSessionTableColumnKeysForViewportWidth(
    effectiveSessionTableLayoutWidthPx(windowInnerWidthPx, sessionTableHostWidthPx),
  );
}

/**
 * CSS `@container session-tables` rules for `[data-session-col]`, generated from
 * {@link SESSION_TABLE_COLUMN_MIN_WIDTH_PX}. The session-tables host uses `container-type: inline-size`
 * so column hiding follows the **rendered** width of that region (layout-enforced).
 */
export function sessionTableResponsiveContainerCss(): string {
  return SESSION_TABLE_COLUMN_REMOVAL_ORDER.map(
    (key) =>
      `@container session-tables (max-width: ${SESSION_TABLE_COLUMN_MIN_WIDTH_PX[key] - 1}px) { [data-session-col="${key}"] { display: none !important; } }`,
  ).join("\n");
}
