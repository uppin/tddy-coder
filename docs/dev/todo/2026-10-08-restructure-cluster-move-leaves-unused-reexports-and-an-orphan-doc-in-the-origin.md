# 2026-10-08 — the origin keeps `pub(crate) use <moved>::*;` lines nothing names any more, and an orphan doc comment

**Category:** Engine failure plus manual fixes (build corrections)
**Source:** #carve 21/21 (PR #536), R9. Related: [2026-10-08-restructure-apply-rustfmt-reorders-unrelated-reexports-in-the-origin-root](2026-10-08-restructure-apply-rustfmt-reorders-unrelated-reexports-in-the-origin-root.md)

After the launch cluster moved, `connection_service.rs` kept eight glob re-exports that the removed siblings used through `super::X` and that nothing names now (`clippy -D warnings`: `unused import`):
`service_util::*`, `agent_roster::*`, `claude_cli_spawn::*`, `managed_launch::*`, `stack_child_spawn::*`, `conversation_spawn::*`, `tddy_session_files::attachment_progress::*`, and a doc-comment line whose module had gone (`empty line after doc comment`).

**Hand fixes:** `claude_cli_spawn::*`, `managed_launch::*` and `attachment_progress::*` deleted (unused even in test builds); `service_util::*`, `agent_roster::*`, `stack_child_spawn::*`, `conversation_spawn::*` gated `#[cfg(test)]` (only lifecycle's own test modules name them);
the orphan doc line deleted.

**What the engine should do:** remove a glob re-export no remaining module names (the tidy removes unused `use` items; these are `pub(crate) use … ::*` and are skipped), and carry a doc comment with the item it documents. Delete this file with that fix.
