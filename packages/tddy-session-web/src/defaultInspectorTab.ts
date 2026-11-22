/**
 * The tab the inspector opens on when the URL names none (`#workqueue` 5/14): **Work queue** when the
 * session owns one, else **PR** when the session's branch has a pull request, otherwise **Worktree**.
 * An explicit `?inspector=` always wins. None of the rules reads a role.
 */
import type { InspectorTabName } from "tddy-client-web";

export interface DefaultTabFacts {
  /** `SessionEntry.owns_work_queue`: the session owns a work queue. */
  ownsWorkQueue: boolean;
  /** `SessionEntry.pr_number`; 0 when the session has no known PR. */
  prNumber: bigint;
}

export function defaultInspectorTab(session: DefaultTabFacts): InspectorTabName {
  if (session.ownsWorkQueue) return "work-queue";
  return session.prNumber > 0n ? "pr" : "worktree";
}
