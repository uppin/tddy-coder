# 2026-10-03 — Transactional groups in restructure plans

**Type:** Feature

`#live-plan` 10/15, PR [#566](https://github.com/uppin/tddy-coder/pull/566). Cross-package entry:
[2026-10-03-transactional-groups.md](../../../../docs/dev/changesets/2026-10-03-transactional-groups.md).
Product entry: [2026-10-03-transactional-groups.md](../../../../docs/ft/coder/changelog/2026-10-03-transactional-groups.md).

Consecutive operations sharing a `"group"` are applied as one unit, gated by `cargo check
--all-targets` at the group's end and rolled back byte for byte when it fails. How it works:
[readiness-and-gates.md](../readiness-and-gates.md#transactional-groups).

- **`plan.rs`:** `RefactorOp.group: Option<String>`; `#[serde(deny_unknown_fields)]` on `RefactorOp`;
  `Plan::parse` refuses non-consecutive members of one group.
- **`journal.rs` / `journal/group.rs`** (new): `OpStatus::{GroupStarted, PreImaged, GroupCompleted,
  GroupRolledBack}`, serde-default `group` / `members` / `pre_images` on `JournalRecord`, `PreImage`
  (`capture`, `restore`), `OpenGroup`, `Journal::open_group`. `group.rs` holds `PreImage`, its `impl` and
  `OpenGroup`; `journal.rs` re-exports it so every path is unchanged.
- **`runner/group_gate.rs`** (new): `GroupRun` (`begin`, `pre_image`, `applied`, `finish`, `enter`,
  `settle`, `roll_back_on_failure`), `GroupGate`, `Settled`, `gate_group`, `roll_back_group`,
  `roll_back_an_open_group`.
- **`runner/entry_points/store_run.rs`** and **`.../store_run/applied_op_record.rs`** (new, holds
  `record_applied_op`, `settle_folded_plans`, `record_resynced_digest`): the command line's apply loop
  enters, settles and rolls back groups, and reports a group's members only once it is kept.
- **`runner/entry_points/check_entry_points.rs`:** `check --deep` merges a group's findings into one.
- **`runner/entry_points/anchor_entry_points.rs`:** a continued run rolls back an open group first.
- **`console.rs`:** `console::group`, the `   group: <name>` line.
- **`lib.rs`:** `RestructureError::GroupDoesNotCompile { group, errors }`.

Both new modules were split off by the engine itself (`extract_module` with `to_file`, `reexport: glob`,
`check --deep` first); the engine emitted `pub(crate) use …::*`, which a public re-export rejects, and the
one-token fix to `pub use` was made by hand (see the cross-package entry).

## Final measurements

| Measure | Result |
|---|---|
| `journal.rs` production lines | 294 → 460 (+ `journal/group.rs`, 65) |
| `store_run.rs` production lines | 471 → 463 (+ `store_run/applied_op_record.rs`, 104) |
| `apply_held_plan` (index daemon) / CLI apply loop | 139 / 151 → 139 / 145 lines |
| Tests | `tests/transactional_groups_acceptance.rs` (live rust-analyzer, `--test-threads=1`) plus unit tests in `plan.rs` and `journal.rs`; scoped checks green |
