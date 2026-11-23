# 2026-11-23 — `#workqueue` sweep: oversized files split

**Type:** Architecture

Behaviour-preserving restructure; no public path changed. `rpc.rs` 514 → 273 (`rpc/service.rs`); `docs.rs` loses the dead document writer and validator; inline test modules of `git_ops`, `item_ops/nodes` and `assess` moved to `tests.rs` children. Details: [cross-package entry](../../../../docs/dev/changesets/2026-11-23-workqueue-sweep.md).
