import { useCallback, useEffect, useRef, useState } from "react";
import type { Client } from "@connectrpc/connect";
import type { BranchResolution, ConnectionService } from "../../../gen/connection_pb";
import type { BranchQuery } from "./branchQueries";

type ConnectionClient = Client<typeof ConnectionService>;

/** Interval (ms) at which the PR-Stack view re-polls `QueryBranch` for each rendered branch. */
export const POLL_INTERVAL_MS = 5000;

export interface QueryBranchState {
  /** The latest resolution per branch. Keyed by branch alone — the base is an input, not an identity. */
  resolutionByBranch: Record<string, BranchResolution>;
  /**
   * Replace one branch's resolution with a fresher one the caller already holds — a pull answers with
   * the branch re-resolved after the refs moved, so the row repaints without waiting for the next
   * tick. The map *is* the state, so there is no override to go stale.
   *
   * The write takes a ticket at the moment it is made, which is after the pull's git operation
   * finished: every poll still in flight was therefore issued against the refs as they stood *before*
   * the pull, and each of those responses is dropped when it lands rather than reinstating the
   * pre-pull comparison for the rest of an interval.
   */
  setResolution: (branch: string, resolution: BranchResolution) => void;
}

/**
 * Polls `QueryBranch` for each of `queries` — immediately on mount and then every
 * {@link POLL_INTERVAL_MS} — and returns a `branch → BranchResolution` map. Each resolution carries
 * the in-progress child session, the on-disk worktree, the live GitHub PR status and the branch's
 * standing against the base the query named, resolved server-side in a single call.
 *
 * A query names both refs because a branch's standing against its base is a comparison; the result is
 * still keyed by branch alone, since a branch has exactly one base at a time and every reader
 * (`startBlockers`, `isNodeOrphaned`, the row's PR/worktree/session lines) asks about the branch.
 *
 * Two things follow from that key, and the hook enforces both rather than leaving them to its callers:
 *
 * - **Writes are ordered, not last-to-land.** Responses arrive out of order — the calls are
 *   independent and a pull's own re-resolution races the polls it overlapped — so every write takes a
 *   monotonic ticket and a response older than the branch's last applied write is dropped.
 * - **A branch's resolution belongs to the base it was resolved against.** When a repoint moves a
 *   node onto a different base, the cached comparison describes a base the node no longer has, and a
 *   pull issued from it would merge the branch it was just moved off. It is dropped the moment the
 *   query changes, so the row states nothing until the new comparison answers.
 *
 * A failed call leaves the previous value for that branch untouched (no crash, no fabricated
 * resolution). The poll set follows `queries`, so only currently-rendered nodes are queried.
 */
export function useQueryBranch(
  client: ConnectionClient | undefined,
  sessionToken: string,
  orchestratorSessionId: string,
  queries: BranchQuery[],
): QueryBranchState {
  const [resolutionByBranch, setResolutions] = useState<Record<string, BranchResolution>>({});
  // A stable dependency for the query set so the effect re-subscribes only when it truly changes —
  // including when a branch keeps its name but its base moves, which is a different comparison.
  const queriesKey = queries.map((q) => `${q.branch} ${q.baseBranch}`).join(" ");
  // The next ticket to hand out, and the ticket of the newest write applied per branch. Refs rather
  // than state: they order the writes, they are never rendered, and a re-render must not reset them.
  const nextTicket = useRef(0);
  const appliedTicketByBranch = useRef<Record<string, number>>({});
  // The base each branch's cached resolution was resolved against, so a changed base is detectable.
  const queriedBaseByBranch = useRef<Record<string, string>>({});

  const takeTicket = () => nextTicket.current++;

  const applyResolution = useCallback(
    (branch: string, resolution: BranchResolution, ticket: number) => {
      // Strictly older than what this branch already shows: the response describes refs that have
      // since moved, and layering it back over the newer answer would un-repaint the row.
      if (ticket < (appliedTicketByBranch.current[branch] ?? -1)) return;
      appliedTicketByBranch.current[branch] = ticket;
      setResolutions((prev) => ({ ...prev, [branch]: resolution }));
    },
    [],
  );

  const setResolution = useCallback(
    (branch: string, resolution: BranchResolution) => {
      applyResolution(branch, resolution, takeTicket());
    },
    [applyResolution],
  );

  useEffect(() => {
    if (!client) return;
    const activeQueries = queries.filter((q) => q.branch.length > 0);
    if (activeQueries.length === 0) return;

    // A branch now measured against a different base: what is cached answers a question that is no
    // longer being asked. The responses still in flight for the old base are dropped by the previous
    // effect's own `cancelled` flag, which React sets before this one runs.
    const staleBranches = activeQueries
      .filter((q) => {
        const previousBase = queriedBaseByBranch.current[q.branch];
        return previousBase !== undefined && previousBase !== q.baseBranch;
      })
      .map((q) => q.branch);
    for (const q of activeQueries) queriedBaseByBranch.current[q.branch] = q.baseBranch;
    if (staleBranches.length > 0) {
      setResolutions((prev) => {
        const next = { ...prev };
        for (const branch of staleBranches) delete next[branch];
        return next;
      });
    }

    let cancelled = false;

    const poll = () => {
      for (const query of activeQueries) {
        // Taken before the call is issued, not when it answers: the response describes the refs as
        // they stood at this moment, so anything written later supersedes it however late it lands.
        const ticket = takeTicket();
        client
          .queryBranch({
            sessionToken,
            sessionId: orchestratorSessionId,
            branch: query.branch,
            baseBranch: query.baseBranch,
          })
          .then((res) => {
            if (cancelled || !res.resolution) return;
            applyResolution(query.branch, res.resolution, ticket);
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
    // `queriesKey` stands in for `queries`; the array identity changes each render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [client, sessionToken, orchestratorSessionId, queriesKey, applyResolution]);

  return { resolutionByBranch, setResolution };
}
