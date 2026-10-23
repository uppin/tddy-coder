import React, { useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { WorktreeService } from "../../gen/worktree_pb";
import { useSessionWorktreeStats } from "../../rpc/useSessionWorktreeStats";
import { hostNotConnected } from "../../rpc/rpcFailure";
import { useActionFailure } from "../../hooks/useActionFailure";
import { InlineError } from "../ui/inline-error";
import { formatDiskBytes } from "./worktreeStatsFormat";
import { formatLastCalculated } from "../../lib/worktreeSize";
import { Button } from "../ui/button";

export interface SessionWorktreeTabProps {
  client: Client<typeof WorktreeService> | null;
  sessionToken: string;
  projectId: string;
  sessionId: string;
  repoPath: string;
}

/**
 * Session Inspector → Worktree tab: the selected session's own worktree — lazy, streamed disk size
 * (None/Calculating/Cached) + diff summary, plus Clear (`git clean -fdx`) / Delete (two-step confirm)
 * and, when the worktree is missing, Restore. Refresh re-triggers the size calculation.
 * A failed action, a failed stats stream and no client at all each show their reason above the
 * tab's content. See docs/ft/web/session-worktree-inspector.md and
 * docs/ft/web/worktree-disk-usage-streaming.md.
 */
export function SessionWorktreeTab({
  client,
  sessionToken,
  projectId,
  sessionId,
  repoPath,
}: SessionWorktreeTabProps) {
  const { row, status, missing, error: statsError, refresh } = useSessionWorktreeStats(
    client,
    sessionToken,
    projectId,
    repoPath,
  );
  const [pendingClear, setPendingClear] = useState(false);
  const [pendingDelete, setPendingDelete] = useState(false);
  const action = useActionFailure();

  function connected() {
    if (!client) throw hostNotConnected(null);
    return client;
  }

  function onConfirmClear() {
    void action.run(async () => {
      await connected().cleanWorktree({ sessionToken, projectId, worktreePath: repoPath });
      setPendingClear(false);
      refresh();
    });
  }

  function onConfirmDelete() {
    void action.run(async () => {
      await connected().removeWorktree({ sessionToken, projectId, worktreePath: repoPath });
      setPendingDelete(false);
      // No recalculation here — the worktree is gone; the daemon's stream drops it from the snapshot.
    });
  }

  function onRestore() {
    void action.run(async () => {
      await connected().restoreSessionWorktree({ sessionToken, projectId, sessionId });
      refresh();
    });
  }

  // The action the operator just took outranks the stream's standing failure.
  const error = action.error ?? statsError;

  return (
    <div
      data-testid="session-worktree-tab"
      className="px-3 py-3 flex flex-col gap-3 text-xs text-muted-foreground"
    >
      <InlineError testId="session-worktree-error" message={error} className="text-xs" />
      {missing ? (
        <div data-testid="session-worktree-missing" className="flex flex-col gap-2">
          <p>This session&apos;s worktree is no longer on disk.</p>
          <div>
            <Button
              type="button"
              size="xs"
              variant="outline"
              data-testid="session-worktree-restore"
              disabled={!client || action.pending}
              onClick={onRestore}
            >
              Restore worktree
            </Button>
          </div>
        </div>
      ) : row ? (
        <>
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
            <dt className="text-muted-foreground">Disk</dt>
            <dd className="text-foreground tabular-nums" data-testid="session-worktree-size">
              {status === "cached" ? formatDiskBytes(row.diskBytes) : "Calculating…"}
            </dd>
            {status === "cached" ? (
              <>
                <dt className="text-muted-foreground">Last calculated</dt>
                <dd
                  className="text-foreground"
                  data-testid="session-worktree-last-calculated"
                >
                  {formatLastCalculated(
                    row.sizeCalculatedAtUnixMs > 0n ? Number(row.sizeCalculatedAtUnixMs) : undefined,
                    Date.now(),
                  )}
                </dd>
              </>
            ) : null}
            <dt className="text-muted-foreground">Branch</dt>
            <dd className="text-foreground" data-testid="session-worktree-branch">
              {row.branchLabel}
            </dd>
            <dt className="text-muted-foreground">Changed files</dt>
            <dd
              className="text-foreground tabular-nums"
              data-testid="session-worktree-changed"
            >
              {row.changedFiles}
            </dd>
          </dl>

          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              size="xs"
              variant="outline"
              data-testid="session-worktree-refresh"
              disabled={!client}
              onClick={refresh}
            >
              Refresh
            </Button>

            <Button
              type="button"
              size="xs"
              variant="outline"
              data-testid="session-worktree-clear"
              disabled={!client}
              onClick={() => {
                setPendingClear(true);
              }}
            >
              Clear
            </Button>
            {pendingClear ? (
              <Button
                type="button"
                size="xs"
                variant="destructive"
                data-testid="session-worktree-clear-confirm"
                disabled={!client || action.pending}
                onClick={onConfirmClear}
              >
                Confirm clear
              </Button>
            ) : null}

            <Button
              type="button"
              size="xs"
              variant="outline"
              data-testid="session-worktree-delete"
              disabled={!client}
              onClick={() => {
                setPendingDelete(true);
              }}
            >
              Delete
            </Button>
            {pendingDelete ? (
              <Button
                type="button"
                size="xs"
                variant="destructive"
                data-testid="session-worktree-delete-confirm"
                disabled={!client || action.pending}
                onClick={onConfirmDelete}
              >
                Confirm delete
              </Button>
            ) : null}
          </div>
        </>
      ) : statsError ? null : (
        <p>Loading worktree stats…</p>
      )}
    </div>
  );
}
