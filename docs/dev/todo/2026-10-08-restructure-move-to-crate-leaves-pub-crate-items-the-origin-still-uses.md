# 2026-10-08 — a cross-crate move leaves `pub(crate)` items the origin still names (E0603 / E0624), widened by hand

**Category:** Manual fix after an engine move (build corrections, visibility only)
**Source:** #carve 21/21 (PR #536), R3: `svc_materialize_staged_attachment` → `tddy-session-files`. Engine cause: item 3 of
[2026-09-09-restructure-defects-from-the-first-cross-crate-move](2026-09-09-restructure-defects-from-the-first-cross-crate-move.md);
same shape as [2026-10-08-restructure-move-to-crate-leaves-a-pub-crate-fn-the-facade-caller-needs](2026-10-08-restructure-move-to-crate-leaves-a-pub-crate-fn-the-facade-caller-needs.md)

## What the engine did vs what was needed

The survey said "1 item(s) reached from outside: AttachmentState". The move left it, its four fields and the methods lifecycle calls
as `pub(crate)`, so `split_ports.rs`, `launch_ports.rs` and `attached_initial_prompt.rs` failed with `E0603`.

## The hand fixes (all `pub(crate)` → `pub`, in `packages/tddy-session-files/src/`)

- `svc_materialize_staged_attachment.rs:29-33`: `struct AttachmentState` and its fields `config`, `tddy_data_dir`, `staging_base_dir`, `peer_routing`.
- `session_attachment_materialization.rs:25` `prepare_session_attachments` and `:38` `materialize_session_attachments` (the two
  methods lifecycle calls; the four other methods stay `pub(crate)` because only these use them).

## What the engine should do

Widen every item the survey lists as reached from outside, and the fields and methods of a reached type that the origin's
remaining code names (the second half is also in [2026-10-04-restructure-move-item-does-not-widen-fields-or-impl-members](2026-10-04-restructure-move-item-does-not-widen-fields-or-impl-members.md)).

## Seen again in R4 (`tddy-session-activity`)

`presenter_observer_spawn.rs`: `PresenterObserverDeps` and its four fields (lines 6-10) and `maybe_spawn_presenter_observer` (line 23),
`pub(crate)` → `pub` (`E0603`, from `handler_state.rs` and `launch_ports.rs`, which stay in lifecycle).
