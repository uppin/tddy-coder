import { useCallback, useEffect, useRef, useState } from "react";
import type { Client } from "@connectrpc/connect";
import { ConnectionService, type WorktreeRow } from "../gen/connection_pb";

/** Client-side refresh cadence for the session Worktree tab — 10 minutes. */
export const WORKTREE_STATS_REFRESH_MS = 10 * 60 * 1000;

export interface UseSessionWorktreeStatsResult {
  /** The session's own worktree row (matched by `repoPath`), or `null` when missing / not loaded. */
  row: WorktreeRow | null;
  /** True once a list response arrived and no row matches `repoPath` — the worktree is gone. */
  missing: boolean;
  /** True until the first list response resolves. */
  loading: boolean;
  /** Trigger an immediate `refresh: true` reload (used by the Refresh button and after actions). */
  refresh: () => void;
}

/**
 * Loads the session's worktree stats from the cache-backed `ListWorktreesForProject` RPC and keeps
 * them fresh: `refresh: false` on mount (instant cached snapshot) and `refresh: true` on a
 * 10-minute timer while mounted. Returns the row whose `path` equals the session's `repoPath`.
 */
export function useSessionWorktreeStats(
  client: Client<typeof ConnectionService> | null,
  sessionToken: string,
  projectId: string,
  repoPath: string,
): UseSessionWorktreeStatsResult {
  const [row, setRow] = useState<WorktreeRow | null>(null);
  const [loaded, setLoaded] = useState(false);
  const cancelledRef = useRef(false);

  const load = useCallback(
    async (refreshFlag: boolean) => {
      if (!client) return;
      try {
        const res = await client.listWorktreesForProject({
          sessionToken,
          projectId,
          refresh: refreshFlag,
        });
        if (cancelledRef.current) return;
        setRow(res.worktrees.find((w) => w.path === repoPath) ?? null);
        setLoaded(true);
      } catch (err) {
        // A list that fails while still mounted leaves the last-known row in place (no fabrication).
        if (!cancelledRef.current) {
          console.debug("[useSessionWorktreeStats] listWorktreesForProject error", err);
        }
      }
    },
    [client, sessionToken, projectId, repoPath],
  );

  const refresh = useCallback(() => {
    void load(true);
  }, [load]);

  useEffect(() => {
    cancelledRef.current = false;
    void load(false);
    const id = setInterval(() => {
      void load(true);
    }, WORKTREE_STATS_REFRESH_MS);
    return () => {
      cancelledRef.current = true;
      clearInterval(id);
    };
  }, [load]);

  return { row, missing: loaded && row === null, loading: !loaded, refresh };
}
