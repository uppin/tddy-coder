/**
 * A list of commits, oldest first (`#workqueue` 4/14). Reused by the PR view's Inbox and Outbox tabs (n11).
 *
 * Test id per row: `session-pr-commit-<sha>`.
 */
import type { PullRequestCommit } from "tddy-client-web/gen/worktree_pb";

export interface CommitListProps {
  commits: PullRequestCommit[];
}

export function CommitList(_props: CommitListProps) {
  // TODO(n4 session-pr-tabs): implement
  return null;
}
