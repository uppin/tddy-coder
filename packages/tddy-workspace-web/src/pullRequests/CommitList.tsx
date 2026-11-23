/**
 * A list of commits, oldest first (`#workqueue` 4/14). Reused by the PR view's Inbox and Outbox tabs (n11).
 *
 * Test id per row: `session-pr-commit-<sha>`.
 */
import React from "react";
import type { PullRequestCommit } from "tddy-client-web/gen/worktree_pb";

export interface CommitListProps {
  commits: PullRequestCommit[];
}

export function CommitList({ commits }: CommitListProps) {
  return (
    <ul className="flex flex-col gap-1">
      {commits.map((commit) => (
        <li
          key={commit.sha}
          data-testid={`session-pr-commit-${commit.sha}`}
          className="flex items-baseline gap-2"
        >
          <code className="text-foreground">{commit.sha.slice(0, 7)}</code>
          <span className="min-w-0 flex-1 truncate">{commit.subject}</span>
          <span>{commit.author}</span>
        </li>
      ))}
    </ul>
  );
}
