# 2026-11-23 — Each work item shows its commit inbox and outbox, with conflicts predicted
**Type:** Feature

`#workqueue` 11/14 ([PR #1012](https://github.com/uppin/tddy-coder/pull/1012)). For an item on a branch,
tddy reads its own commits, its **inbox** (what its line ancestors have that it lacks, each commit
attributed to the nearest ancestor or the base, with a per-item `git merge-tree` conflict prediction) and
its **outbox** (per successor, what that successor lacks). It reads remote tips after a fetch and touches no
checkout (`tddy_work_queue::commit_flow`). The cache holds one entry per item, keyed by remote tips.

RPCs `GetWorkItemInbox`, `GetWorkItemOutbox`, `GetWorkQueueCommitCounts`
(`tddy-daemon-rpc::work_queue::commit_flow`); tools `work_item_inbox` / `work_item_outbox`
(`tddy-tool-engine::work_item_commit_flow_tools`, dispatched through the same module). The session PR view
mounts Inbox and Outbox tabs (`SessionPullRequestFlowTabs`), and queue rows show counts and a conflict
marker (`workItemCommitCounts`). The closed tool lists follow.

Packages: tddy-work-queue, tddy-service, tddy-tool-engine, tddy-daemon-rpc, tddy-tools, tddy-client-web,
tddy-workspace-web, tddy-session-web, tddy-workqueue-web.

Wrap: rebased onto n10. Validation fixed an inbox cache that never evicted. Stack-shared files over
budget, measured but not split: `work_queue/ports.rs` 903 → 925, `test_util.rs` 612 → 643.
