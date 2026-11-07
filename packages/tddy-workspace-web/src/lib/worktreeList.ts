/**
 * View model for the Worktrees screen's sidebar: a worktree is listed by its name, biggest on-disk
 * size first, with the last size the operator saw kept (and shown as stale) while a fresh one is
 * being calculated or the daemon has not yet answered.
 *
 * Feature: docs/ft/web/worktree-disk-usage-streaming.md
 */

import type { WorktreeStatsRow } from "./worktreeSize";

/** One worktree as the sidebar lists it. */
export interface WorktreeListItem {
  path: string;
  /** The worktree's directory name — what the operator calls it; the path is detail. */
  name: string;
  branch: string;
  /** Last known size; absent only when this worktree has never been sized. */
  diskBytes?: bigint;
  /** Unix epoch (ms) of the calculation `diskBytes` came from. */
  calculatedAtUnixMs?: number;
  /** A size walk is pending or running for this worktree. */
  calculating: boolean;
  /** `diskBytes` is a remembered value that a fresh calculation has not yet replaced. */
  stale: boolean;
}

/** The directory name of a worktree path (the last non-empty segment). */
export function worktreeName(path: string): string {
  const segments = path.split("/").filter((s) => s !== "");
  return segments[segments.length - 1] ?? path;
}

/** Biggest first; worktrees with no known size sink to the bottom, ties broken by name. */
export function sortWorktreesBySize(items: readonly WorktreeListItem[]): WorktreeListItem[] {
  return [...items].sort((a, b) => {
    if (a.diskBytes !== undefined && b.diskBytes !== undefined && a.diskBytes !== b.diskBytes) {
      return a.diskBytes > b.diskBytes ? -1 : 1;
    }
    if ((a.diskBytes === undefined) !== (b.diskBytes === undefined)) {
      return a.diskBytes === undefined ? 1 : -1;
    }
    return a.name.localeCompare(b.name);
  });
}

/** How far a round of size calculations has got. */
export function sizingProgress(items: readonly WorktreeListItem[]): {
  done: number;
  total: number;
} {
  return { done: items.filter((i) => !i.calculating).length, total: items.length };
}

/** What is remembered of one worktree between visits. */
export interface RememberedWorktree {
  path: string;
  branch: string;
  diskBytes?: bigint;
  calculatedAtUnixMs?: number;
}

/**
 * Lay the daemon's rows over what was remembered: a size the daemon has not (re)calculated yet
 * falls back to the remembered one, flagged stale. With no daemon rows yet, the remembered
 * worktrees are listed as they were.
 */
export function mergeWorktreeItems(
  streamed: readonly WorktreeStatsRow[],
  remembered: readonly RememberedWorktree[],
): WorktreeListItem[] {
  const memory = new Map(remembered.map((r) => [r.path, r]));
  if (streamed.length === 0) {
    return remembered.map((r) => ({
      path: r.path,
      name: worktreeName(r.path),
      branch: r.branch,
      diskBytes: r.diskBytes,
      calculatedAtUnixMs: r.calculatedAtUnixMs,
      calculating: false,
      stale: r.diskBytes !== undefined,
    }));
  }
  return streamed.map((row) => {
    const fresh = row.status === "cached" && row.diskBytes !== undefined;
    const old = memory.get(row.path);
    return {
      path: row.path,
      name: worktreeName(row.path),
      branch: row.branch,
      diskBytes: fresh ? row.diskBytes : old?.diskBytes,
      calculatedAtUnixMs: fresh ? row.calculatedAtUnixMs : old?.calculatedAtUnixMs,
      calculating: row.status !== "cached",
      stale: !fresh && old?.diskBytes !== undefined,
    };
  });
}

const STORAGE_PREFIX = "tddy.worktrees.sizes.";

interface StoredWorktree {
  path: string;
  branch: string;
  diskBytes?: string;
  calculatedAtUnixMs?: number;
}

/** The worktrees remembered for a project; empty when nothing is stored or storage is unusable. */
export function readRememberedWorktrees(projectId: string): RememberedWorktree[] {
  try {
    const raw = window.localStorage.getItem(STORAGE_PREFIX + projectId);
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return (parsed as StoredWorktree[]).map((s) => ({
      path: s.path,
      branch: s.branch,
      diskBytes: s.diskBytes !== undefined ? BigInt(s.diskBytes) : undefined,
      calculatedAtUnixMs: s.calculatedAtUnixMs,
    }));
  } catch {
    return [];
  }
}

/** Remember a project's worktrees for the next visit. Storage being unusable only loses the memory. */
export function writeRememberedWorktrees(
  projectId: string,
  items: readonly WorktreeListItem[],
): void {
  const stored: StoredWorktree[] = items.map((i) => ({
    path: i.path,
    branch: i.branch,
    diskBytes: i.diskBytes?.toString(),
    calculatedAtUnixMs: i.calculatedAtUnixMs,
  }));
  try {
    window.localStorage.setItem(STORAGE_PREFIX + projectId, JSON.stringify(stored));
  } catch {
    // Quota or a blocked store: the daemon's own persisted sizes still serve the next visit.
  }
}
