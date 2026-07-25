/**
 * Streaming hook for the Worktrees manager screen's lazy, per-worktree disk usage.
 *
 * Subscribes once to `ConnectionService.StreamWorktreeStats` for a project over the shared
 * common-room LiveKit connection (`useDaemonClient`, like `useHostStats`). The daemon emits a first
 * snapshot frame carrying every worktree with its size status, then one frame per worktree as its
 * size finishes (Calculating → Cached). Each frame is folded into a stable row list via
 * `applyWorktreeStatsEvent`.
 *
 * `recalculateAll()` re-subscribes with `recalculate_all: true` (re-enqueues every worktree);
 * `refresh()` re-subscribes for a fresh snapshot (e.g. after a delete); `calculate(path)` triggers a
 * single worktree's (re)calculation via the unary `CalculateWorktreeSize` — its result surfaces on
 * the open stream.
 *
 * Feature: docs/ft/web/worktree-disk-usage-streaming.md
 */

import { useCallback, useEffect, useState } from "react";
import {
  ConnectionService,
  WorktreeSizeStatus,
  type WorktreeRow,
} from "../gen/connection_pb";
import { useDaemonClient } from "./selectedDaemon";
import { useAuthContext } from "../hooks/authProvider";
import { formatDiskBytes } from "../components/sessions/worktreeStatsFormat";
import {
  applyWorktreeStatsEvent,
  type WorktreeSizeStatus as DomainSizeStatus,
  type WorktreeStatsRow,
  type WorktreeStatsStreamEvent,
} from "../lib/worktreeSize";

export interface UseWorktreeStatsStreamResult {
  /** Current worktree rows, folded from the stream. Empty until the first snapshot arrives. */
  rows: WorktreeStatsRow[];
  /** Re-subscribe with `recalculate_all`, re-enqueuing a size walk for every worktree. */
  recalculateAll: () => void;
  /** Re-subscribe for a fresh snapshot without forcing recalculation (e.g. after a delete). */
  refresh: () => void;
  /** Trigger a (re)calculation for one worktree by path; the result surfaces on the stream. */
  calculate: (path: string) => void;
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

function rowFromRpc(w: WorktreeRow): WorktreeStatsRow {
  const cached = w.sizeStatus === WorktreeSizeStatus.CACHED;
  const calculatedAt = Number(w.sizeCalculatedAtUnixMs);
  return {
    path: w.path,
    branch: w.branchLabel,
    status: domainStatus(w.sizeStatus),
    sizeLabel: cached ? formatDiskBytes(w.diskBytes) : undefined,
    diskBytes: cached ? w.diskBytes : undefined,
    changedFiles: w.changedFiles,
    linesAdded: Number(w.linesAdded),
    linesRemoved: Number(w.linesRemoved),
    calculatedAtUnixMs: calculatedAt > 0 ? calculatedAt : undefined,
  };
}

function eventFromRpc(event: {
  snapshot: WorktreeRow[];
  updated?: WorktreeRow;
}): WorktreeStatsStreamEvent {
  // A frame carries either an updated single row or a full snapshot — never both.
  if (event.updated) {
    return { updated: rowFromRpc(event.updated) };
  }
  return { snapshot: event.snapshot.map(rowFromRpc) };
}

/**
 * Subscribe to `StreamWorktreeStats` for `projectId` and expose the folded rows plus the
 * recalculate/calculate controls. Re-subscribes whenever the daemon client, session token, project,
 * or the requested subscription (recalculate flag + nonce) changes.
 */
export function useWorktreeStatsStream(projectId: string): UseWorktreeStatsStreamResult {
  const client = useDaemonClient(ConnectionService);
  const { sessionToken } = useAuthContext();
  const [rows, setRows] = useState<WorktreeStatsRow[]>([]);
  const [subscription, setSubscription] = useState({ recalculateAll: false, nonce: 0 });

  const token = sessionToken ?? "";
  const trimmedProjectId = projectId.trim();
  const { recalculateAll: recalculateAllFlag, nonce } = subscription;

  useEffect(() => {
    if (!client || !trimmedProjectId) {
      setRows([]);
      return;
    }
    let cancelled = false;
    // A new subscription always starts from an empty list — the daemon's first frame is a snapshot.
    setRows([]);

    (async () => {
      try {
        for await (const event of client.streamWorktreeStats({
          sessionToken: token,
          projectId: trimmedProjectId,
          recalculateAll: recalculateAllFlag,
        })) {
          if (cancelled) break;
          const mapped = eventFromRpc(event);
          setRows((prev) => applyWorktreeStatsEvent(prev, mapped));
        }
      } catch (err) {
        // A stream aborted on unmount/re-subscribe surfaces as an AbortError; ignore it. Any other
        // error while still mounted leaves the last-known rows in place (no fallback fabrication).
        if (!cancelled) {
          console.debug("[useWorktreeStatsStream] streamWorktreeStats error", err);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [client, token, trimmedProjectId, recalculateAllFlag, nonce]);

  const recalculateAll = useCallback(() => {
    setSubscription((s) => ({ recalculateAll: true, nonce: s.nonce + 1 }));
  }, []);

  const refresh = useCallback(() => {
    setSubscription((s) => ({ recalculateAll: false, nonce: s.nonce + 1 }));
  }, []);

  const calculate = useCallback(
    (path: string) => {
      if (!client || !trimmedProjectId) return;
      void client
        .calculateWorktreeSize({ sessionToken: token, projectId: trimmedProjectId, worktreePath: path })
        .catch((err) => {
          console.debug("[useWorktreeStatsStream] calculateWorktreeSize error", err);
        });
    },
    [client, token, trimmedProjectId],
  );

  return { rows, recalculateAll, refresh, calculate };
}
