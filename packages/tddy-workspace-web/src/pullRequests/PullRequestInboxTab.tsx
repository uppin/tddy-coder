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

export function PullRequestInboxTab(_props: { inbox: InboxCommitView[] }) {
  // TODO(n11 commit-inbox-outbox): implement
  return null;
}

export function PullRequestOutboxTab(_props: { outbox: OutboxView[] }) {
  // TODO(n11 commit-inbox-outbox): implement
  return null;
}
