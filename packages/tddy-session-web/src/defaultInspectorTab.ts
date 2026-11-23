/**
 * The tab the inspector opens on when the URL names none: **PR** when the session's branch has a pull
 * request, otherwise **Worktree**. A session's work queue is a sidebar, not an inspector tab.
 * An explicit `?inspector=` always wins. None of the rules reads a role.
 */
import type { InspectorTabName } from "tddy-client-web";

export interface DefaultTabFacts {
  /** `SessionEntry.owns_work_queue`: the session owns a work queue (shown in its sidebar; no longer picks the tab). */
  ownsWorkQueue: boolean;
  /** `SessionEntry.pr_number`; 0 when the session has no known PR. */
  prNumber: bigint;
}

export function defaultInspectorTab(session: DefaultTabFacts): InspectorTabName {
  return session.prNumber > 0n ? "pr" : "worktree";
}
