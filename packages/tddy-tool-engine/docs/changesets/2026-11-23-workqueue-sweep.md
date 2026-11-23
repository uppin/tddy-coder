# 2026-11-23 — `#workqueue` sweep: oversized files split

**Type:** Architecture

Behaviour-preserving restructure; no public path changed. `work_queue_tools.rs` 531 → 368 (`work_queue_tools/args.rs`); `dynamic_proxy.rs` and the test modules moved out of `lib.rs` and the two tool modules. Details: [cross-package entry](../../../../docs/dev/changesets/2026-11-23-workqueue-sweep.md).
