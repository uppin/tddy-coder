# `move_module_to_crate` refuses whenever any file left behind names the module

**Category:** refusal
**Operation:** `move_module_to_crate`, `move_cluster_to_crate`
**Measured:** `#carve` 5/11, against `7a7f043d` (master tip `d5a157cc`, i.e. with `#carve` 1/10's
nested-move and facade-cycle fixes present)

## What happens

`restructure check --deep` refuses every move in a plan that takes a shared DTO module out of
`tddy-core`, with one finding per file that stays behind and names it:

```
0: `packages/tddy-core/src/backend/claude.rs` stays behind in `packages/tddy-core` and names
   `workflow::ids`, which operation 0 moves to `packages/tddy-workflow` — a path re-pointed at a
   crate the module holding it is not in. Move `packages/tddy-core/src/backend/claude.rs` with the
   set, or leave `workflow::ids` where it is
```

Four moves produced **19 findings**: 17 for `workflow/ids.rs` and 2 for `backend_questions`.

## Why it is wrong

The remedy the refusal offers is not available, and the condition it refuses on is the normal case.

- **"Move the file with the set"** — the files named are `backend/claude.rs`, `backend/codex.rs`,
  `changeset/model.rs`, `workflow/engine.rs`, `session_chain.rs`, `test_support.rs` and eleven
  others. They are the *callers*. Moving every caller of `GoalId` into `tddy-workflow` would move
  most of `tddy-core`.
- **"Leave the module where it is"** — that is a refusal to perform the operation.
- The re-point it objects to is **legal**: `tddy-core` depends on `tddy-workflow`, so a file staying
  in `tddy-core` may name `tddy_workflow::GoalId` perfectly well.
- With `reexport: "glob"` the re-point is not even needed — the origin keeps a facade, so
  `crate::workflow::ids::GoalId` in a file left behind keeps resolving untouched. The refusal fires
  regardless of `reexport`.

A module worth moving to a shared crate is, by definition, one that other modules name. This
refusal triggers on exactly that property, so the operation cannot perform the move it exists for.

## Scope

Confirmed on `#carve` 5/11 over four modules (`workflow/ids.rs`, `presenter/events.rs`,
`backend_questions`, `stream/progress.rs`), planned as one `move_module_to_crate` plus one
`move_cluster_to_crate`. All four were completed by hand (`git mv` + a hand-written glob facade)
in `5bdf140a`.

Independently recorded earlier on `#unbundle` node 2, where 0 of ~24 candidate modules moved. This
is the second stack to plan around the operation and the second to fall back to `git mv`.

## Suggested fix

Do not treat a left-behind caller as a blocker when either
(a) the origin crate already depends on the destination, so the re-point compiles, or
(b) `reexport` leaves a facade, so no re-point is required at all.
Reserve the refusal for the case it was written for: a destination that would have to depend on the
crate it just left.
