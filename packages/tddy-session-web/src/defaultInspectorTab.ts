/**
 * The tab the inspector opens on when the URL names none (`#workqueue` 4/14): **PR** when the session's
 * branch has a pull request, otherwise **Worktree**. An explicit `?inspector=` always wins. A later
 * node adds a first rule (Work queue, for a session that owns one); none of the rules reads a role.
 */
import type { InspectorTabName } from "tddy-client-web";

export interface DefaultTabFacts {
  /** `SessionEntry.pr_number`; 0 when the session has no known PR. */
  prNumber: bigint;
}

export function defaultInspectorTab(session: DefaultTabFacts): InspectorTabName {
  return session.prNumber > 0n ? "pr" : "worktree";
}
