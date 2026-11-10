/**
 * Client-side model for lazy, streamed worktree disk usage.
 *
 * The daemon streams worktree stats as an initial snapshot followed by
 * per-worktree updates as sizes finish calculating. These helpers fold that
 * stream into a stable row list and format the "last calculated" label.
 *
 * Feature: docs/ft/web/worktree-disk-usage-streaming.md
 */

/** Size-calculation state for a single worktree. */
export type WorktreeSizeStatus = "none" | "calculating" | "cached";

/** One worktree's stats as surfaced in the Worktrees screen. */
export interface WorktreeStatsRow {
  path: string;
  branch: string;
  status: WorktreeSizeStatus;
  /** Formatted disk size (e.g. "1.2 GB"); present once cached. */
  sizeLabel?: string;
  /** Raw disk size in bytes; present once cached. */
  diskBytes?: bigint;
  changedFiles: number;
  linesAdded: number;
  linesRemoved: number;
  /** Unix epoch (ms) of the last successful size calculation. */
  calculatedAtUnixMs?: number;
  /** Pre-formatted relative "last calculated" label. */
  lastCalculatedLabel?: string;
}

/** One frame of the worktree-stats stream: a full snapshot or a single update. */
export interface WorktreeStatsStreamEvent {
  snapshot?: WorktreeStatsRow[];
  updated?: WorktreeStatsRow;
}

/**
 * Fold one stream event into the current rows, returning a new array.
 *
 * - A `snapshot` replaces the entire row set.
 * - An `updated` row replaces the row with a matching `path` (preserving order
 *   and siblings), or is appended when the path is not yet known.
 */
export function applyWorktreeStatsEvent(
  rows: WorktreeStatsRow[],
  event: WorktreeStatsStreamEvent,
): WorktreeStatsRow[] {
  if (event.snapshot) {
    return [...event.snapshot];
  }

  const updated = event.updated;
  if (!updated) {
    return [...rows];
  }

  const index = rows.findIndex((row) => row.path === updated.path);
  if (index === -1) {
    return [...rows, updated];
  }

  const next = [...rows];
  next[index] = updated;
  return next;
}

const MINUTE_MS = 60_000;
const HOUR_MS = 3_600_000;
const DAY_MS = 86_400_000;

/**
 * Format the relative age of a worktree's last size calculation.
 *
 * `undefined` → "never"; otherwise the coarsest unit that fits: "just now"
 * (< 1 min), "N min ago" (< 1 hr), "N hr ago" (< 1 day), else "N d ago".
 */
export function formatLastCalculated(
  calculatedAtUnixMs: number | undefined,
  nowMs: number,
): string {
  if (calculatedAtUnixMs === undefined) {
    return "never";
  }

  const diff = nowMs - calculatedAtUnixMs;
  if (diff < MINUTE_MS) {
    return "just now";
  }
  if (diff < HOUR_MS) {
    return `${Math.floor(diff / MINUTE_MS)} min ago`;
  }
  if (diff < DAY_MS) {
    return `${Math.floor(diff / HOUR_MS)} hr ago`;
  }
  return `${Math.floor(diff / DAY_MS)} d ago`;
}
