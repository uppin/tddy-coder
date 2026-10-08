# 2026-10-08 — a module with a directory child is stranded by `move_module_to_crate`; the cluster workaround flattens it and leaves a dangling self re-export

**Category:** Engine failure plus a manual fix (build correction)
**Source:** #carve 21/21 (PR #536), R3: `svc_materialize_staged_attachment` (child `svc_materialize_staged_attachment/session_attachment_materialization.rs`)
to `tddy-session-files`. Engine causes already filed:
[2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind](2026-10-04-restructure-move-cluster-to-crate-leaves-a-modules-directory-children-behind.md),
[2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling](2026-09-25-restructure-move-to-crate-leaves-a-nested-modules-parent-glob-dangling.md)

## What the engine did

1. One `move_module_to_crate` of the parent (the check said `no findings`) moved `svc_materialize_staged_attachment.rs` and left
   `svc_materialize_staged_attachment/session_attachment_materialization.rs` in lifecycle: `E0583: file not found for module
   session_attachment_materialization` at the destination. Rolled back (git restore, journal removed).
2. The workaround, still the engine: one `move_cluster_to_crate` naming the parent as anchor and the child in `also`. It moved
   both, but as **two siblings at the destination root** (`src/session_attachment_materialization.rs` beside
   `src/svc_materialize_staged_attachment.rs`), declared in `lib.rs`, and left in the moved parent, line 1:
   `pub use tddy_session_files::session_attachment_materialization;` — the parent's old `mod` line rewritten to a facade onto
   the destination's own crate, i.e. onto itself (`E0432`).

## The hand fix

`packages/tddy-session-files/src/svc_materialize_staged_attachment.rs`: delete lines 1-2 (the self re-export and its blank
line). Nothing else; the child is already `pub mod` at the crate root, and the parent's `crate::…` paths already resolve.

## What the engine should do

Move a module's directory children with it (or refuse, naming them, at `check --deep`), keeping the nesting; and never write a
facade for a `mod` line whose target is moving into the same crate as the declaring file. One red test for each.

## Why deferred

The engine is not owned by this stack; the developer's 2026-10-08 ruling accepts a filed hand correction.

## Seen again in R4 (`tddy-session-activity`)

`move_cluster_to_crate` anchored on `presenter_observer_task` with its child `presenter_observer_spawn`, `presenter_intent_client` and
`session_notification_publishing` in `also` (the child must be named, or it is stranded, as in R3) flattened the child to a
root-level sibling and left `pub use tddy_session_activity::presenter_observer_spawn;` at line 8 of the moved parent
(`E0432`). **Hand fix: `tddy_session_activity::` → `crate::` on that line** (kept rather than deleted: lifecycle's
`handler_state.rs:23` and `launch_ports.rs:41` name `crate::presenter_observer_task::presenter_observer_spawn::PresenterObserverDeps`,
and the re-export keeps that path resolving without editing them).
