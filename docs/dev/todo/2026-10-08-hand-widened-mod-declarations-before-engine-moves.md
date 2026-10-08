# 2026-10-08 — `pub(crate) mod x;` was widened to `pub mod x;` by hand so the engine would move the module

**Category:** Manual fix after an engine failure (hand workaround, developer-consented 2026-10-08)
**Source:** #carve 21/21 (PR #536). Engine cause:
[2026-10-08-restructure-move-to-crate-does-not-read-a-restricted-mod-declaration](2026-10-08-restructure-move-to-crate-does-not-read-a-restricted-mod-declaration.md)

## What the engine did vs what was needed

`move_module_to_crate` / `move_cluster_to_crate` refuse a module whose declaration is `pub(crate) mod x;`
(`declares no mod x`). The only way past it was a hand edit of the declaring line to `pub mod x;`, in its own commit
before the engine move. The move then rewrote the line itself (to a `pub use <receiver>::x;` facade), so the widening
does not survive; it only exists in the pre-move commit.

## Each hand edit

| Module | File and line | Edit | Commit |
|---|---|---|---|
| `daemon_hook_urls` | `packages/tddy-session-lifecycle/src/connection_service.rs:568` | `pub(crate) mod` → `pub mod` | R1 pre-move commit |

## What the engine should do so this is automatic

`crate_move/manifest_edits.rs::module_declaration` should accept any visibility (`pub`, `pub(crate)`, `pub(super)`,
`pub(in path)`) before `mod`, and the facade writer should keep the module's narrower visibility when the move leaves a
`pub use`. Then this file and its sibling engine todo are deleted together.

## Why this was deferred

No node of the `#carve` stack owns the engine; the developer ruled that a hand workaround, filed and kept in its own
commit, is acceptable to keep #536 moving.

R4 added two rows (commit "widen two mod declarations to pub mod …"):

| `presenter_observer_spawn` | `packages/tddy-session-lifecycle/src/presenter_observer_task.rs:8` | `pub(crate) mod` → `pub mod` | R4 pre-move commit |
| `session_notification_publishing` | `packages/tddy-session-lifecycle/src/session_notifications.rs:14` | `pub(crate) mod` → `pub mod` | R4 pre-move commit |
