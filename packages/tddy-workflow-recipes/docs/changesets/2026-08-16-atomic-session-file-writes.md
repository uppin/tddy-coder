# 2026-08-16 — **atomic session-file writes

**Type:** Bug Fix

recipe persistence goes through `write_atomic`** — `writer.rs`, `pr_stack/hooks.rs`, `plan_pr_stack/hooks.rs`, `orchestrate_pr_stack/{hooks,transient}.rs`, `tdd/{hooks,interview}.rs`, `tdd_small/hooks.rs`, `bugfix/interview.rs` and `review/persist.rs` no longer truncate in place, so a write that cannot complete leaves the previous state readable instead of a 0-byte file the next goal reads as absent. [tddy-core architecture.md § Atomic file writes](../../../tddy-core/docs/architecture.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-workflow-recipes)
