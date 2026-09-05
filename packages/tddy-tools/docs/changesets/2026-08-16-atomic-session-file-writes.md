# 2026-08-16 — **atomic session-file writes

**Type:** Bug Fix

`session_context.rs` goes through `write_atomic`** — the in-jail session context is replaced by swap-file-plus-rename, so a failed write leaves the previous context readable rather than empty. [tddy-core architecture.md § Atomic file writes](../../../tddy-core/docs/architecture.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-tools)
