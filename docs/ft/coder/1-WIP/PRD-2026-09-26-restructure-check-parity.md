# `check --deep` refuses the cross-crate moves `apply` cannot build - PRD

**Date**: 2026-09-26
**PRD Type**: Bug Fix

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — `check` findings
  for `move_module_to_crate` / `move_cluster_to_crate`; § Known limitations ("the header pass is
  mechanical").

## Summary

`check --deep` is the gate a planner is told to trust before an apply. Two recorded cases pass it and
then fail to build: a moved module that reaches a module staying behind **only through body paths**,
and a move whose module name the destination **already defines**. This PRD adds both as findings, so
the plan is refused before anything is written.

## Background

- [`…-check-misses-a-body-path-to-a-module-staying-behind.md`](../../dev/todo/2026-09-25-restructure-check-misses-a-body-path-to-a-module-staying-behind.md)
  — `workspace_session.rs` reaches `crate::connection_service::…` only in bodies; the stays-behind
  finding reads headers; `check --deep` said `no findings`, the move would be `E0433`/`E0603`.
- [`…-check-misses-a-module-name-the-destination-already-has.md`](../../dev/todo/2026-09-25-restructure-check-misses-a-module-name-the-destination-already-has.md)
  — the destination root already declares (or has a file for) the moved module's name.

## Proposed Changes

### What's Changing

1. **Stays-behind from the path survey.** The stays-behind finding reads the moved file's paths from
   the `move-paths` node's survey — headers and bodies — so a body path to a module staying behind
   is a finding. When that module is one the plan could never move (it is the crate's host of the
   moved code, not a sibling), the remedy says so rather than suggesting `move_cluster_to_crate`.
2. **Destination name collision.** A static finding, needing no index: the destination root already
   binds the moved module's name (a `mod` declaration, or a file at the target path). The finding
   calls it a **merge**, which no operation performs.

### What's Staying the Same

- The survey itself and the apply-side rewrites (`move-paths` node owns them).
- Every other finding.

## Impact Analysis

### Technical Impact

`packages/tddy-code-restructuring/src/crate_move.rs` / `crate_move/` findings (`check` path);
`tests/check_precondition_parity.rs` with two-crate fixtures.

### User Impact

`check --deep` stops passing plans that cannot build, for both recorded shapes.

## Acceptance Criteria

- [ ] A moved file reaching `crate::host::f(…)` only in a body, with `host` staying behind, is a
      `check --deep` finding naming the path and the line; nothing is written.
- [ ] The finding's remedy does not suggest a cluster when the module staying behind is the host.
- [ ] A move whose module name the destination root already declares, or has a file for, is a
      finding naming it as a merge; `check` (without `--deep`) reports it too.
- [ ] A plan free of both shapes is still `no findings`.

## References

- [Rust code restructuring](../rust-code-restructuring.md)
