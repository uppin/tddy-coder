/**
 * The session inspector's PR tab (`#workqueue` 4/14): the PR whose head is the session's branch — its
 * state, checks, threaded comments and commits — streamed from the daemon's GitHub inbox through
 * `WorktreeService.StreamSessionPullRequest`. Reused for a work item's PR view by later nodes.
 *
 * Test ids: `session-pr-tab`, `session-pr-state`, `session-pr-check-<name>`, `session-pr-thread-<root id>`,
 * `session-pr-comment-<id>`, `session-pr-commit-<sha>`, `session-pr-none`.
 */
import type { Client } from "@connectrpc/connect";
import type { WorktreeService } from "tddy-client-web/gen/worktree_pb";

export interface SessionPullRequestTabProps {
  client: Client<typeof WorktreeService>;
  sessionToken: string;
  projectId: string;
  sessionId: string;
}

export function SessionPullRequestTab(_props: SessionPullRequestTabProps) {
  // TODO(n4 session-pr-tabs): implement — stream the view, render state, checks, threads, commits.
  return null;
}
