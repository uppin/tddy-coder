# 2026-11-23 — `#workqueue` sweep: oversized files split

**Type:** Architecture

Behaviour-preserving restructure; no public path changed. `service.rs` 897 → 679: the inherent `impl WorktreeServiceImpl` in `service/host.rs`; still over budget. Details: [cross-package entry](../../../../docs/dev/changesets/2026-11-23-workqueue-sweep.md).
