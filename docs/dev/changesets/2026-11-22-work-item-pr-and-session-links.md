# 2026-11-22 — A work item takes an existing PR and linked sessions
**Type:** Feature

`#workqueue` 6/14 ([PR #1007](https://github.com/uppin/tddy-coder/pull/1007)). A work item records an
**assigned PR** (`WorkItem.pr`: number, URL, head branch — read from GitHub as the project's account, never
the caller's claim) and **linked sessions** (`WorkItem.linked_sessions`). Four operations — assign,
unassign, link, unlink — are daemon exec tools for the queue's owner (`work_item_assign_pr`,
`work_item_unassign_pr`, `work_item_link_session`, `work_item_unlink_session`) and RPCs behind the queue
row's controls. The PR's head branch becomes the item's branch; a different recorded branch is refused,
naming both. A linked session is Addressed by the queue. `StreamWorkQueueStatus` pushes each item's PR
state and check counts (running, failed, succeeded) from the GitHub inbox — a snapshot, then one per
`Pr`/`Checks` event in the queue's repository; the row shows them for a PR'ed item, and lists each run with n4's `CheckRow` when expanded
(`tddy-workqueue-web` now depends on `tddy-workspace-web`).

Moved here from n2 by the operator: `GetPrStatus` / `QueryBranch`'s PR leg (previously the caller's
login-keyed token over `repo_root`) and `work_queue_status` (previously the synchronous `GithubPrApi`)
read through the inbox with the project's credential (`tddy_work_queue::inbox_reads`), the status path
rewritten async.

Packages: tddy-changeset, tddy-work-queue, tddy-service, tddy-tool-engine, tddy-daemon-rpc, tddy-tools,
tddy-agent-launch, tddy-workqueue-web, tddy-client-web (and generated TS in tddy-rust-typescript-tests).

Wrap gaps closed: `pull_number_for_node` prefers the assigned PR; the expanded check rows (planned,
missing at green) — `WorkItemStatus.checks`. Wrap cleanups: `WorkQueueView.tsx` split (512 → 432 lines, link state into `useWorkItemLinksState.ts`);
`WorkQueueHandler`'s new methods made required; `ItemLinkError` gained `Display` (the tools had answered
its `Debug`); the stream's snapshot moved to `inbox_reads::work_queue_status_view`, and a queue deleted
mid-view ends the stream with the store's reason.

Backlog: new `2026-11-22-work-queue-status-stream-is-local-only.md` (the stream is not forwarded to a
peer). n2's `2026-11-22-github-inbox-reads-a-repository-with-one-projects-credential.md` kept open and
updated — n6 made the caller-path reads real, but closing it changes n2's inbox (rows per credential,
`RepoReaders` keyed by project), outside this PR's boundaries.

Code issues: `oversized-file-work-queue-ports` touched, unchanged (860 → 860). New
`oversized-file-test-util` (`tddy-daemon-rpc/src/test_util.rs`, 500 → 552): not split — the file is
stack-shared with every `#workqueue` node, so the split waits for the post-stack sweep.

Feature docs: [pr-stacking.md § An item's assigned PR and linked sessions](../../ft/coder/pr-stacking.md#an-items-assigned-pr-and-linked-sessions-added-2026-11-22),
[prstack-package.md](../../ft/web/prstack-package.md).
