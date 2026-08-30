import type { SessionEntry } from "../gen/connection_pb";

/**
 * The em-dash `session_list_enrichment` puts in `workflow_goal` when it cannot report a real one —
 * an unreadable `.session.yaml`, or a workflow session whose `changeset.yaml` is unreadable or does
 * not list it (`SessionListStatusDisplay::all_placeholders`). `ListSessions` ships it to the browser
 * verbatim, so without this rule such a row would be labelled "—", which names nothing.
 *
 * Note it is *not* what a claude-cli session reports: that session type takes an earlier branch and
 * yields an empty `workflow_goal`, which the priority chain already treats as absent.
 */
const WORKFLOW_GOAL_PLACEHOLDER = "—";

/**
 * Derives a human-readable display label for a session in the drawer.
 *
 * Priority:
 * 1. Basename of `repoPath` (when non-empty and yields a non-empty segment).
 * 2. `workflowGoal` (when non-empty and not the display placeholder).
 * 3. First 8 characters of `sessionId` as a last-resort fallback.
 *
 * `packages/tddy-core/src/session_label.rs` mirrors this rule so a Telegram notification and a
 * drawer row name the same session identically (PRD FR1,
 * docs/ft/daemon/session-notifications.md). The two must stay in
 * step: a change here without the matching change there splits the naming again.
 */
export function sessionDrawerLabel(session: SessionEntry): string {
  const trimmedRepo = session.repoPath.trim();
  if (trimmedRepo !== "" && trimmedRepo !== "/") {
    const basename = trimmedRepo.split("/").filter(Boolean).at(-1);
    if (basename && basename !== "") {
      return basename;
    }
  }

  const trimmedGoal = session.workflowGoal.trim();
  if (trimmedGoal !== "" && trimmedGoal !== WORKFLOW_GOAL_PLACEHOLDER) {
    return trimmedGoal;
  }

  return session.sessionId.slice(0, 8);
}
