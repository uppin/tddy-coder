# 2026-11-23 — A work queue knows its lineage
**Type:** Feature

`#workqueue` 9/14 ([PR #1010](https://github.com/uppin/tddy-coder/pull/1010)). A queue now has two kinds of
edge, kept apart. **Line edges** are read, never stored: each item's open PR base through the daemon's GitHub
inbox, resolved to another item, the trunk or an external branch (`tddy_work_queue::lineage::LineEndpoint`);
an item without a readable PR shows its planned base. **Dependency edges** are stored on the item
(`WorkItem.needs`), acyclic — `set_dependencies` refuses an unknown item or a cycle (naming it) and writes
nothing; `detect_cycle` is exposed for waves.

Owner tools `work_queue_set_dependencies` / `work_queue_graph` (`tddy-tool-engine::work_queue_lineage_tools`)
and RPCs `SetWorkQueueDependencies` / `GetWorkQueueGraph` (`tddy-daemon-rpc::work_queue::lineage`); the exec
tool's answers live in `exec_tool/work_queue_tools/ops/lineage_ops.rs`. Optional `gh stack` membership probe
(`tddy_github::gh_stack::gh_stack_membership`). The Work queue tab draws the graph (`WorkQueueGraph`,
`WorkQueueGraphPanel`): line edges solid, dependency edges dashed.

Packages: tddy-changeset, tddy-work-queue, tddy-github, tddy-tool-engine, tddy-service, tddy-daemon-rpc,
tddy-tools, tddy-agent-launch, tddy-client-web, tddy-workqueue-web.

Wrap: rebased onto n7/n8 (n9 was cut from n6); a need named twice is now stored once (order-preserving
dedup); `ops.rs` kept under budget (525 → 483) by moving the lineage answers out. Stack-shared files over
budget, measured not split: `work_queue/ports.rs` 881 → 895, `test_util.rs` 582 → 602.

Backlog: added `docs/dev/todo/2026-11-23-work-queue-graph-is-not-live.md` (graph redraws on read, not on
inbox events).
