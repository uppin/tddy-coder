# 2026-10-03 — The apply loop gates and rolls back transactional groups

**Type:** Feature

`#live-plan` 10/15, PR [#566](https://github.com/uppin/tddy-coder/pull/566). Cross-package entry:
[2026-10-03-transactional-groups.md](../../../../docs/dev/changesets/2026-10-03-transactional-groups.md).

- **`proto/code_index.proto`:** `OperationApplied.group = 9`, the group an operation belongs to, empty
  when ungrouped.
- **`src/apply.rs`:** `apply_held_plan` carries a `GroupRun` (`tddy_code_restructuring::runner::group_gate`)
  through the loop, as the command line's loop does. A group's `OperationApplied` events are sent once the
  group compiled; a failure while a group is open rolls it back; a cancelled token between members rolls
  the open group back at once; a cancel during the group's own check leaves it open for a resume.
- **`src/status.rs`:** `RestructureError::GroupDoesNotCompile` maps to `FailedPrecondition`.
- **`src/render.rs`:** a single-shot run prints `   group: <name>` after an applied operation in a group
  (`console::group`); `tddy-tools`' `index_console.rs` does the same.

Described in [code-index-service.md](../docs/code-index-service.md#transactional-groups). The apply loop's
`apply_held_plan` is 139 lines, the same as before groups.
