# 2026-10-08 — `move_item` of an `impl DaemonSessionHost` block rewrote `super::service_util::…` to `super::tddy_session_split::service_util::…`

**Category:** Engine failure plus manual fixes (build corrections)
**Source:** #carve 21/21 (PR #536), host-block node: three `move_item` ops (reexport `outside`, `name` = a new wiring module) over the `<DaemonSessionHost>` blocks of
`session_worktree_observer.rs`, `session_acting_identity.rs` and `conversation_worktree_op.rs`. Related: [2026-10-04-restructure-move-item-copies-the-whole-use-header](2026-10-04-restructure-move-item-copies-the-whole-use-header.md)

## What the engine did

`check --deep` said `no findings`; the apply moved the three blocks and failed its compile gate on two errors in the new `svc_session_identity_wiring.rs`:
`super::service_util::find_registered_project(…)` had been rewritten to `super::tddy_session_split::service_util::find_registered_project(…)`
(`E0433`) — the path went through lifecycle's facade `pub use tddy_session_split::service_util;` (R8) and the engine spelled the facade's *target crate* as if it were a child of `super`.
Because the gate failed, the apply's unused-import tidy never ran, so each moved block also left its source module with the imports it no longer used (17 `unused_imports` errors under `-D warnings`).

## Hand fixes (build corrections)

- `svc_session_identity_wiring.rs`: `super::tddy_session_split::service_util::` → `super::service_util::` (what was written before the move).
- `cargo fix --allow-dirty --lib --tests -p tddy-session-lifecycle` removed the 17 unused imports (machine edit, not hand-typed), then `cargo fmt`.

## What the engine should do

Spell a path through a facade the way it was written (or as the defining crate's path), never `super::<crate>::`; and run the unused-import tidy even when the compile gate fails on something else, or say that it did not run. Delete this file with those fixes.
