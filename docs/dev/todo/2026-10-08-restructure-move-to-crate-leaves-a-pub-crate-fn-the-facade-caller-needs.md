# 2026-10-08 — a cross-crate move leaves a `pub(crate) fn` the origin still calls (E0603), fixed by hand

**Category:** Manual fix after an engine move (build correction)
**Source:** #carve 21/21 (PR #536), R2: `move_module_to_crate` of `first_admission_token` to `tddy-daemon-livekit`.
Engine cause: item 3 of [2026-09-09-restructure-defects-from-the-first-cross-crate-move](2026-09-09-restructure-defects-from-the-first-cross-crate-move.md)

## What the engine did vs what was needed

The move left `pub(crate) fn mint_first_admission_token` in `packages/tddy-daemon-livekit/src/first_admission_token.rs:9`.
Its one caller stayed behind (`connection_service/svc_provision_agent_clone.rs:60`), so the apply's compile gate failed
with `E0603: function mint_first_admission_token is private`. The edits were left on disk, as designed.

## The hand fix

`first_admission_token.rs:9`: `pub(crate) fn` → `pub fn`. Visibility only; nothing else in R2 needed a correction.

## What the engine should do

When a moved module is reached from the origin (the survey already lists it: "1 item(s) reached from outside:
mint_first_admission_token"), widen each reached item that is `pub(crate)`/private to `pub` in the destination as part
of the move, and report it, as `move_item` already does for same-crate moves. Delete this file with that fix.

## Why deferred

Same reason as the first-move todo: the engine is not owned by any node of this stack; a one-line visibility edit is
the permitted build correction.
