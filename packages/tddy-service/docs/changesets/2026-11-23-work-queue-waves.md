# 2026-11-23 — A work queue derives its waves, line and readiness
**Type:** Feature

`#workqueue` 10/14 ([PR #1011](https://github.com/uppin/tddy-coder/pull/1011)). From the stored dependency
edges a queue now derives, never stores: waves, the blockers-first line, each item's binding needs and the
first item its own order steps back at (`tddy_work_queue::waves`). A merged need is met — stored status or
the live PR read through the inbox — so a merged item no longer shows as waited on.

Owner tool `work_queue_waves` (`tddy-tool-engine::work_queue_waves_tools`), RPC `GetWorkQueueWaves`
(`tddy-daemon-rpc::work_queue::waves`), and `wave` / `binding_needs` on `WorkItemStatus` and in
`work_queue_status`. The Work queue tab draws wave lanes, readiness text and a line-check banner
(`WaveLanes`). Closed tool lists and the project-home prompt follow.

Packages: tddy-work-queue, tddy-service, tddy-tool-engine, tddy-daemon-rpc, tddy-tools, tddy-agent-launch,
tddy-client-web, tddy-workqueue-web.

Wrap: rebased onto n9 (cut from n6 until then). `ops.rs` kept under budget (483 → 491) by putting the waves
answer in `ops/waves_ops.rs`. Stack-shared files over budget, measured not split: `work_queue/ports.rs`
895 → 903, `test_util.rs` 602 → 612.
