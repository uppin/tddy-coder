# 2026-11-23 — A work queue flags commits leaked from ancestor PRs and items that are not linear
**Type:** Feature

`#workqueue` 12/14 ([PR #1013](https://github.com/uppin/tddy-coder/pull/1013)). Per item with an open PR:
**leaks** (commits of its PR that belong to a line ancestor, by SHA or as a rebased copy, naming the nearest
ancestor) and **linearity** (`linear`, `behind` N, `conflicting`, `unknown`), computed by
`tddy_work_queue::chain_health` from the GitHub inbox and the owner's clone
(`chain_health_reads::queue_chain_health`). `WorkItemStatus` gains `leaks` and `line`; `work_queue_status`
adds `chain_health` per node; queue rows show both chips (`WorkItemChainHealth`).

Packages: tddy-work-queue, tddy-service, tddy-daemon-rpc, tddy-tool-engine, tddy-session-worktree
(`count_divergence` made `pub`), tddy-client-web, tddy-workqueue-web.

Wrap: rebased onto n11's wrapped head. The status stream builds on n11's `work_queue_status_view` and
overlays chain health after it. No file crossed 500 production lines.
