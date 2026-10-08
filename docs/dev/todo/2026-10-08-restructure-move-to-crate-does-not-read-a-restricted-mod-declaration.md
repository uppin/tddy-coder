# 2026-10-08 — `move_module_to_crate` / `move_cluster_to_crate` refuse a module declared `pub(crate) mod x;`

**Category:** Restructure engine defect
**Source:** #carve 21/21 (PR #536), R1 (`tddy-daemon-kernel`) and R4 (`tddy-session-activity`) of
[2026-09-26-carve-lifecycle-moves](../1-WIP/2026-09-26-carve-lifecycle-moves.md)

## What the engine did

`restructure check --deep` (warm index) on a plan whose anchor is a module declared with a restricted
visibility refuses it, whatever the module holds:

```text
1: plan is malformed: packages/tddy-session-lifecycle/src/connection_service.rs declares no `mod daemon_hook_urls` — a module this parent module does not declare is not this crate's to move
1: plan is malformed: packages/tddy-session-lifecycle/src/session_notifications.rs declares no `mod session_notification_publishing` — a module this parent module does not declare is not this crate's to move
```

The declarations are real: `connection_service.rs:568` is `pub(crate) mod daemon_hook_urls;` and
`session_notifications.rs` has `pub(crate) mod session_notification_publishing;`. The same plan shape on
`mod first_admission_token;` (plain) and on `pub mod x;` is accepted.

## Where it is

`packages/tddy-code-restructuring/src/crate_move/manifest_edits.rs`, `module_declaration` (L6): it trims the
line, strips the prefix `"pub "` and compares the rest to `mod <module>;`. `pub(crate) mod x;`,
`pub(super) mod x;` and `pub(in path) mod x;` are never equal to it, so `move_preconditions`
(`crate_move/preconditions.rs`, L200ff) reports the module as undeclared. Any consumer of
`module_declaration` (the facade writer, the origin edit) inherits the same blind spot.

## What the node did about it

Nothing: the plan was not worked around. Widening the declaration by hand to `pub mod` before the move would
make the engine accept it, but it is a hand edit the changeset's Boundaries do not allow (hand edits are
build corrections after a move) and the developer's rule is that a refusal means stop and ask.
R1's other module (`agent_list_mapping`, declared `pub mod`) was moved; `daemon_hook_urls` and
`session_notification_publishing` are the blocked ones. Both are also reached by other milestones' clusters
(`presenter_observer_task` names `session_notification_publishing`), so R4 is blocked as a whole.

## Why deferred

The fix is in the engine, not in lifecycle, and no node of the `#carve` stack owns the engine. A red test in
`crate_move/manifest_edits.rs`'s test module (a `pub(crate) mod x;` and a `pub(super) mod x;` line each returning
the line's span) is the whole acceptance.
