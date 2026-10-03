# 2026-10-03 — Transactional groups in restructure plans

**Type:** Feature

`#live-plan` 10/15 — PR [#566](https://github.com/uppin/tddy-coder/pull/566),
`feature/live-plan/transactional-groups`. Product entry:
[2026-10-03-transactional-groups.md](../../ft/coder/changelog/2026-10-03-transactional-groups.md).

| Package | Entry |
|---|---|
| `tddy-code-restructuring` | [transactional-groups](../../../packages/tddy-code-restructuring/docs/changesets/2026-10-03-transactional-groups.md) |
| `tddy-index-daemon` | [transactional-groups](../../../packages/tddy-index-daemon/docs/changesets/2026-10-03-transactional-groups.md) |

`tddy-tools` gains only the `   group: <name>` line in `index_console.rs`.

## What changed

Some refactors cannot compile step by step. A plan now marks consecutive operations with the same
`"group"`; they are applied as one unit. `cargo check --all-targets` runs at the group's end over the
packages the group touched, and a group that fails there is rolled back exactly from journalled
pre-images (edited files rewritten, created files removed, renames undone). The run stops with
`RestructureError::GroupDoesNotCompile` (`FailedPrecondition`) naming the group and the compiler's
errors; everything before the group stays applied.

- **Plan.** `RefactorOp.group`; members must be consecutive; `RefactorOp` refuses unknown fields so a
  misspelt `group` cannot run ungrouped.
- **Journal.** `group_started`, per-member `pre_imaged` (written before the member's `in_flight`),
  `group_completed` / `group_rolled_back`; resume inside a group rolls the partial group back and applies
  it again whole.
- **Both apply loops** (the command line's and the index daemon's) share `runner::group_gate::GroupRun`.
  Any failure inside a group rolls the whole group back and returns the original error; cancellation
  leaves it open for a resume (the daemon rolls back immediately between members). A group's members reach
  the plan store together, at its end.
- **Reporting.** A group's members are reported only once the group is kept: `OperationApplied.group`
  (proto field 9) for the daemon, a `   group: <name>` line for the console.
- **`check --deep`** reports one finding per group with a refused member.
- Ungrouped runs are unchanged: the end-of-run gate and the edits left on disk after a failure.

## Decisions

- `stop_after` is judged where a group would begin; all of a group's members count toward the limit.
- The ledger checkpoint is written before `group_rolled_back` is journalled, so a crash between the two
  stays resumable.
- A rolled-back group removes a file it created but leaves an empty directory it was created in.
- Tests use a stand-in: no operation today yields a tree that compiles only after a later operation, so
  the passing group is two renames and every failing group fails through `move_test_binary_to_crate`
  leaving its `include_str!` file behind. When signature rewrites land, a true break-then-repair pair
  should replace the stand-in.

## Restructuring done by the engine

Both splits were `tddy-tools restructure apply` (one `extract_module` each, `to_file`, `reexport: glob`,
`check --deep` first); no moved code was written by hand. The engine emitted `pub(crate) use …::*`, which
the public re-exports reject; changing it to `pub use` and deleting three imports the engine left unused in
the new file (its tidy does not run when the compile gate fails) were the only hand edits. The defect is
recorded in the backlog as *restructure glob re-export is narrower than the moved items need*
(2026-10-03).

## Final measurements

| File | Production lines before | after |
|---|---:|---:|
| `tddy-code-restructuring/src/journal.rs` | 294 | 460 |
| `tddy-code-restructuring/src/journal/group.rs` (new) | — | 65 |
| `tddy-code-restructuring/src/runner/entry_points/store_run.rs` | 471 | 463 |
| `tddy-code-restructuring/src/runner/entry_points/store_run/applied_op_record.rs` (new) | — | 104 |

`apply_held_plan`: daemon 139 → 139 lines, command line 151 → 145. Scoped checks green for
`tddy-code-restructuring`, `tddy-index-daemon` and `tddy-tools`; the full workspace is CI's.

## Backlog

`restructure` has no operations that change a function's signature (2026-09-24) stays open: groups are what
make the caller-breaking signature changes usable, and the operations themselves belong to
`signature-rewrites`. *Leftovers from the transactional-groups review* (2026-10-03) records the review
findings and the deferred clean-code refactor. Neither the stale-member refusal nor an index-daemon group
test is covered here; the stale-member refusal waits on live plans' stale-operation detection being green.

The code-issue records of the three packages were re-measured: none describes a file this change split
or measured over budget, so none was closed or narrowed.
