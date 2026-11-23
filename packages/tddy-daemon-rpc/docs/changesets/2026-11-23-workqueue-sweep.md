# 2026-11-23 — `#workqueue` sweep: oversized files split

**Type:** Architecture

Behaviour-preserving restructure; no public path changed. `ports.rs` 970 → 277 (seven bodies in `branch_reads.rs`, `item_writes.rs`, `repoint_rpc.rs`, `pull_base_rpc.rs`); `test_util.rs` 706 → 391; `exec_tool/work_queue_tools.rs` 647 → 312; `ops.rs` 510 → 349; `work_item_git_tools.rs` 529 → 290; `land.rs` tests moved to `land/tests.rs`. Details: [cross-package entry](../../../../docs/dev/changesets/2026-11-23-workqueue-sweep.md).
