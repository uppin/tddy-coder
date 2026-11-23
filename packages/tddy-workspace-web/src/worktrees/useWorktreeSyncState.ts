/**
 * The session worktree's sync state for the inspector's Worktree tab: dirty files, the branch's
 * relation to its upstream, and the Pull (fast-forward) and Push actions that change it.
 *
 * Read through `WorktreeService.GetWorktreeSyncState`; `pull()` / `push()` call `PullWorktree` /
 * `PushWorktree`, which answer the state after the operation. A failed read or action is reported as
 * `error` with the last good state left in place; no client at all is reported, never faked.
 */
import { useCallback, useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { WorktreeService, WorktreeSyncState } from "tddy-client-web/gen/worktree_pb";
import { hostNotConnectedText, rpcFailureText } from "tddy-client-web";

export interface UseWorktreeSyncStateResult {
  /** The latest state, or `null` before the first read. */
  state: WorktreeSyncState | null;
  /** Why the last read or action failed (or that no client reaches the host); `null` otherwise. */
  error: string | null;
  /** True while a pull or push is in flight. */
  acting: boolean;
  pull: () => void;
  push: () => void;
}

export function useWorktreeSyncState(
  client: Client<typeof WorktreeService> | null,
  sessionToken: string,
  projectId: string,
  sessionId: string | undefined,
): UseWorktreeSyncStateResult {
  const [state, setState] = useState<WorktreeSyncState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [acting, setActing] = useState(false);

  useEffect(() => {
    setState(null);
    if (sessionId === undefined) {
      setError(null);
      return;
    }
    if (!client) {
      setError(hostNotConnectedText(null));
      return;
    }
    setError(null);
    let cancelled = false;
    client
      .getWorktreeSyncState({ sessionToken, projectId, sessionId })
      .then((next) => {
        if (!cancelled) setState(next);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(rpcFailureText(err));
      });
    return () => {
      cancelled = true;
    };
  }, [client, sessionToken, projectId, sessionId]);

  const act = useCallback(
    (operation: "pullWorktree" | "pushWorktree") => {
      if (sessionId === undefined) return;
      if (!client) {
        setError(hostNotConnectedText(null));
        return;
      }
      setActing(true);
      client[operation]({ sessionToken, projectId, sessionId })
        .then((next) => {
          setState(next);
          setError(null);
        })
        .catch((err: unknown) => setError(rpcFailureText(err)))
        .finally(() => setActing(false));
    },
    [client, sessionToken, projectId, sessionId],
  );

  const pull = useCallback(() => act("pullWorktree"), [act]);
  const push = useCallback(() => act("pushWorktree"), [act]);

  return { state, error, acting, pull, push };
}
