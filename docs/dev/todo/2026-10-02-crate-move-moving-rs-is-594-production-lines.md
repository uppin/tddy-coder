# 2026-10-02 — `crate_move/moving.rs` is 594 production lines after #541

**Category:** Technical debt (oversized file; budget 500)
**Source:** `#live-plan` 4/7, [#541](https://github.com/uppin/tddy-coder/pull/541), `/pr-wrap` file-length gate
**Record:** [`oversized-file-crate-move-moving.md`](../../packages/tddy-code-restructuring/docs/code-issues/oversized-file-crate-move-moving.md)

## What happened

#541 grew the file from 349 to 594 production lines by adding the grouped-facade writer next to
the `Move` type.

## Why it was deferred

The parent PR `move-paths` also touches the file; splitting it mid-stack turns every layer into a
conflict. Extract the facade writer to its own module once the stack has landed.
