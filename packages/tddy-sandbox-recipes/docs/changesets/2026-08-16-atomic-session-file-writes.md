# 2026-08-16 — **atomic session-file writes

**Type:** Bug Fix

recipe file writes go through `write_atomic`** — `claude_cli.rs` and `cursor_cli.rs` persist through `tddy_core::atomic_file` (adding a `tddy-core` path dependency) rather than truncating in place. [tddy-core architecture.md § Atomic file writes](../../../tddy-core/docs/architecture.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-sandbox-recipes)
