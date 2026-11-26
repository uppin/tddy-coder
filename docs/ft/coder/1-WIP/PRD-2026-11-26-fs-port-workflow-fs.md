# PRD: Workflow, changeset and task crates go through the FS port

**Date:** 2026-11-26 · **Type:** Technical refactor · **Streak:** `#fs-port` n10 of 13

## Summary
Migrate the production disk I/O of `tddy-workflow-recipes`, `tddy-changeset`, `tddy-workflow-engine`, `tddy-workflow`, `tddy-tool-engine`, `tddy-task`, `tddy-actions` onto the `tddy-fs` port (n1),
so their disk-dependent behaviour is testable with `MemFs` and every atomic write goes through the port.
No behaviour, path, format or permission changes.

## Acceptance criteria
- [ ] No direct `std::fs` / `tokio::fs` in production code of the listed crates (tests and `build.rs` may keep real disk).
- [ ] Each crate's disk-touching entry points take an `Arc<dyn Fs>` (or a struct holding one); composition sites pass `DiskFs`.
- [ ] At least one behaviour test per crate runs on `MemFs` without a temp dir.
- [ ] Existing tests of the listed crates pass.
