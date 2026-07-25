/**
 * Streaming hook for the Session Inspector → Worktree tab.
 *
 * Subscribes to `ConnectionService.StreamWorktreeStats` for the session's project (mirroring
 * `useHostStats` — a `for await` over the passed daemon client with a `cancelled` cleanup) and
 * selects the row whose `path` equals the session's `repoPath`. The daemon emits a first snapshot
 * frame carrying every worktree with its lazy size status, then one frame per worktree as its size
 * finishes (Calculating → Cached); subscribing also lazily enqueues a size walk for a `None`
 * worktree. `refresh()` re-triggers a single (re)calculation for this session's worktree via the
 * unary `CalculateWorktreeSize`; its result surfaces on the open stream.
 *
 * Feature: docs/ft/web/worktree-disk-usage-streaming.md
 */

import { useCallback, useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import {
  ConnectionService,
  WorktreeSizeStatus,
  type WorktreeRow,
  type WorktreeStatsEvent,
} from "../gen/connection_pb";
import type { WorktreeSizeStatus as DomainSizeStatus } from "../lib/worktreeSize";

export interface UseSessionWorktreeStatsResult {
  /** The session's own worktree row (matched by `repoPath`), or `null` when missing / not loaded. */
  row: WorktreeRow | null;
  /** The row's lazy size lifecycle state (`"none"` when there is no row yet). */
  status: DomainSizeStatus;
  /** True once a snapshot arrived and no row matches `repoPath` — the worktree is gone. */
  missing: boolean;
  /** True until the first stream frame resolves. */
  loading: boolean;
  /** Re-trigger a size (re)calculation for this session's worktree via `CalculateWorktreeSize`. */
  refresh: () => void;
}

function domainStatus(status: WorktreeSizeStatus): DomainSizeStatus {
  switch (status) {
    case WorktreeSizeStatus.CALCULATING:
      return "calculating";
    case WorktreeSizeStatus.CACHED:
      return "cached";
    default:
      return "none";
  }
}

/**
 * Fold one stream frame into the current rows. A `snapshot` frame replaces the whole set; an
 * `updated` frame replaces the row with the matching `path` (or appends it when unknown).
 */
function foldRows(rows: WorktreeRow[], event: WorktreeStatsEvent): WorktreeRow[] {
  if (event.updated) {
    const updated = event.updated;
    const index = rows.findIndex((row) => row.path === updated.path);
    if (index === -1) {
      return [...rows, updated];
    }
    const next = [...rows];
    next[index] = updated;
    return next;
  }
  return [...event.snapshot];
}

/**
 * Subscribe to `StreamWorktreeStats` for `projectId` and return the session's worktree row (matched
 * by `repoPath`) plus its size status. Re-subscribes whenever the client, session token, or project
 * changes.
 */
export function useSessionWorktreeStats(
  client: Client<typeof ConnectionService> | null,
  sessionToken: string,
  projectId: string,
  repoPath: string,
): UseSessionWorktreeStatsResult {
  const [rows, setRows] = useState<WorktreeRow[]>([]);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (!client) return;
    let cancelled = false;
    // A fresh subscription starts empty — the daemon's first frame is a full snapshot.
    setRows([]);
    setLoaded(false);

    (async () => {
      try {
        for await (const event of client.streamWorktreeStats({
          sessionToken,
          projectId,
          recalculateAll: false,
        })) {
          if (cancelled) break;
          setRows((prev) => foldRows(prev, event));
          setLoaded(true);
        }
      } catch (err) {
        // A stream aborted on unmount/re-subscribe surfaces as an AbortError; ignore it. Any other
        // error while still mounted leaves the last-known rows in place (no fallback fabrication).
        if (!cancelled) {
          console.debug("[useSessionWorktreeStats] streamWorktreeStats error", err);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [client, sessionToken, projectId]);

  const refresh = useCallback(() => {
    if (!client) return;
    void client
      .calculateWorktreeSize({ sessionToken, projectId, worktreePath: repoPath })
      .catch((err) => {
        console.debug("[useSessionWorktreeStats] calculateWorktreeSize error", err);
      });
  }, [client, sessionToken, projectId, repoPath]);

  const row = rows.find((w) => w.path === repoPath) ?? null;
  const status = row ? domainStatus(row.sizeStatus) : "none";

  return { row, status, missing: loaded && row === null, loading: !loaded, refresh };
}
