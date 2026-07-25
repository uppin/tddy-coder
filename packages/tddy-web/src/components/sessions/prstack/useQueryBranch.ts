import { useEffect, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { BranchResolution, ConnectionService } from "../../../gen/connection_pb";

type ConnectionClient = Client<typeof ConnectionService>;

/** Interval (ms) at which the PR-Stack view re-polls `QueryBranch` for each rendered branch. */
export const POLL_INTERVAL_MS = 5000;

/**
 * Polls `QueryBranch` for each of `branches` — immediately on mount and then every
 * {@link POLL_INTERVAL_MS} — and returns a `branch → BranchResolution` map. Each resolution carries
 * the in-progress child session, the on-disk worktree, and the live GitHub PR status for that
 * branch, resolved server-side in a single call.
 *
 * A failed call leaves the previous value for that branch untouched (no crash, no fabricated
 * resolution). The poll set follows `branches`, so only currently-rendered nodes are queried.
 */
export function useQueryBranch(
  client: ConnectionClient | undefined,
  sessionToken: string,
  orchestratorSessionId: string,
  branches: string[],
): Record<string, BranchResolution> {
  const [resolutions, setResolutions] = useState<Record<string, BranchResolution>>({});
  // A stable dependency for the branch set so the effect re-subscribes only when it truly changes.
  const branchesKey = branches.join(" ");

  useEffect(() => {
    if (!client) return;
    const activeBranches = branches.filter((b) => b.length > 0);
    if (activeBranches.length === 0) return;

    let cancelled = false;

    const poll = () => {
      for (const branch of activeBranches) {
        client
          .queryBranch({ sessionToken, sessionId: orchestratorSessionId, branch })
          .then((res) => {
            if (cancelled || !res.resolution) return;
            setResolutions((prev) => ({ ...prev, [branch]: res.resolution! }));
          })
          .catch(() => {
            // Leave the previous value in place; a transient failure must not clear the row.
          });
      }
    };

    poll();
    const id = setInterval(poll, POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
    // `branchesKey` stands in for `branches`; the array identity changes each render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, sessionToken, orchestratorSessionId, branchesKey]);

  return resolutions;
}
