/**
 * The PR view's Inbox and Outbox tabs (`#workqueue` 11/14), beside Commits in n4's view.
 *
 * Inbox: what the item's line ancestors have that it lacks — trunk/external sources first — each commit
 * with its source and a conflict prediction (files named when it conflicts). Outbox: per successor,
 * what it lacks of this item.
 *
 * Test ids: `pr-inbox-commit-${sha}` (`data-source`, `data-conflict="true|false"`),
 * `pr-inbox-conflict-files-${sha}`, `pr-outbox-successor-${itemId}`, `pr-outbox-commit-${itemId}-${sha}`.
 */
import React from "react";

export interface InboxCommitView {
  sha: string;
  subject: string;
  /** "item:n1", "trunk:master" or "external:feat/x". */
  source: string;
  conflictFiles: string[];
}

export interface OutboxView {
  successor: string;
  commits: { sha: string; subject: string }[];
}

const shortSha = (sha: string) => sha.slice(0, 7);

export function PullRequestInboxTab({ inbox }: { inbox: InboxCommitView[] }) {
  if (inbox.length === 0) {
    return <p data-testid="pr-inbox-empty">Nothing missing: this branch has every commit of its base.</p>;
  }
  return (
    <ul data-testid="pr-inbox" className="flex flex-col gap-1">
      {inbox.map((commit) => {
        const conflicts = commit.conflictFiles.length > 0;
        return (
          <li
            key={commit.sha}
            data-testid={`pr-inbox-commit-${commit.sha}`}
            data-source={commit.source}
            data-conflict={conflicts ? "true" : "false"}
            className="flex flex-col"
          >
            <div className="flex items-baseline gap-2">
              <code className="text-foreground">{shortSha(commit.sha)}</code>
              <span className="min-w-0 flex-1 truncate">{commit.subject}</span>
              <span>{commit.source}</span>
              {conflicts ? <strong>conflicts</strong> : null}
            </div>
            {conflicts ? (
              <span data-testid={`pr-inbox-conflict-files-${commit.sha}`}>{commit.conflictFiles.join(", ")}</span>
            ) : null}
          </li>
        );
      })}
    </ul>
  );
}

export function PullRequestOutboxTab({ outbox }: { outbox: OutboxView[] }) {
  if (outbox.length === 0) {
    return <p data-testid="pr-outbox-empty">Nothing owed: every successor has all of this branch.</p>;
  }
  return (
    <div data-testid="pr-outbox" className="flex flex-col gap-2">
      {outbox.map((entry) => (
        <section key={entry.successor} data-testid={`pr-outbox-successor-${entry.successor}`}>
          <h4 className="font-medium text-foreground">{entry.successor}</h4>
          <ul className="flex flex-col gap-1">
            {entry.commits.map((commit) => (
              <li key={commit.sha} data-testid={`pr-outbox-commit-${entry.successor}-${commit.sha}`}>
                <code className="text-foreground">{shortSha(commit.sha)}</code> {commit.subject}
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}
