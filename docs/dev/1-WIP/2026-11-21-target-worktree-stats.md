# Changeset: The Worktree tab shows a target session's worktree

**Date**: 2026-11-21
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — worktree-service streaming with asynchronous remote measurements (ordering: local rows first), remote I/O seam, possibly a web hint
**Stack**: `#target-parity` 7/7 — branch `feature/target-parity/target-worktree-stats`, base `feature/target-parity/question-answer` — PR [#975](https://github.com/uppin/tddy-coder/pull/975)

## Initial Discovery

[2026-11-21-target-worktree-stats-initial-discovery.md](./2026-11-21-target-worktree-stats-initial-discovery.md)

## Affected Packages

- **tddy-worktree-service**: target rows in `StreamWorktreeStats` and in the size calculator, through n2's port
- **tddy-workspace-web**: none expected (the tab already matches its row by path); a hint only if review asks

## Related Feature Documentation

- PRD: [PRD-2026-11-21-target-worktree-stats.md](../../ft/web/1-WIP/PRD-2026-11-21-target-worktree-stats.md)

## Summary

The project's worktree stream gains one row per target session of the project, measured on its target (branch, `git diff --shortstat`, `du`) and delivered as `updated` events, so the local snapshot is never delayed.

## Responsibility

- Target rows: discovery from the project's target sessions, measurement through the port, `stale` for an unreachable target.
- `CalculateWorktreeSize` of a target path.

## Boundaries

- Does not change the three actions (n2) or the proto.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n2` target-worktree-actions | `TargetShells`, the target session of a path | measures rows through them | change the port or the lookup |

## Draft PR contract

No new public surface beyond n2's; the first push is the failing tests below.

## Green wave

**Wave:** 2 of 2
**Greenable independently:** no — measures through n2's port and lookup
**Concurrent with:** n6
**Blocks:** none

Real dependency edges (the line is n1 · n2 · n3 · n4 · n5 · n6 · n7):

    n1 permission-answer → n6 question-answer
    n2 target-worktree-actions → n7 target-worktree-stats

Waves: 1 = n1, n2, n3, n4, n5 · 2 = n6, n7. n1 and n2 lead wave 1 (one dependent each; n1 first as
the more foundational); n3–n5 block nothing.

## Prerequisites

### ⚠ DURING — [oversized-file-service.md](../../../packages/tddy-worktree-service/docs/code-issues/oversized-file-service.md), [oversized-file-worktrees.md](../../../packages/tddy-worktree-service/docs/code-issues/oversized-file-worktrees.md)

New code in its own module; neither file grows.

## Scope

- [x] Target rows in the stream and the calculator
- [ ] Feature docs `session-worktree-inspector.md`, `worktree-disk-usage-streaming.md` updated through the wrap

## Technical Changes

### State A (Current)

`stream_worktree_stats` (`service.rs:658`) lists the local clone's `git worktree list` only; a target session's tab is `missing`.

### State B (Target)

The stream holds the target session's row, measured on the target, after the local snapshot.

## Acceptance Tests

`packages/tddy-worktree-service/tests/target_worktree_stats_acceptance.rs` (`StandInSshTarget` through n2's port; its setup is duplicated from n2's `target_worktree_actions_acceptance.rs` — for the sweep node to share):
- [x] `the_stream_holds_a_target_sessions_worktree_with_its_branch_diff_and_size`
- [x] `the_local_snapshot_is_sent_before_the_target_answers` — a **guard** (a target that takes 5 s a command): green today, must stay green once target rows are measured
- [x] `an_unreachable_targets_row_is_stale_without_numbers`
- [x] `calculating_a_target_worktrees_size_measures_it_on_the_target`

## TODO

- [x] Record initial discovery (`2026-11-21-target-worktree-stats-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (delegated: the user said "finish the rest without approval")
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] Validate tests (/validate-tests)
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap
