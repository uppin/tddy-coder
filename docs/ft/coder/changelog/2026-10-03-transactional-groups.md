# 2026-10-03 — Transactional groups: operations that only compile together

`#live-plan` 10/15, PR [#566](https://github.com/uppin/tddy-coder/pull/566). Feature:
[Rust code restructuring](../rust-code-restructuring.md); the daemon's side:
[Warm code-intelligence daemon](../warm-code-intelligence-daemon.md).

A refactor that changes a type and then adapts every use cannot compile after its first step. A plan can
now give consecutive operations the same `"group"`:

- The group is applied as one unit and checked with `cargo check --all-targets` once, at its end.
- A group that does not compile is rolled back exactly — edited files restored, created files removed,
  renames undone — and the run stops naming the group and the compiler's errors. Everything before the
  group stays applied.
- A crash inside a group, then `--resume`, rolls the partial group back and applies it again.
- A group's operations are reported as applied only once the group is kept, each with a `   group: <name>`
  line; the daemon's `OperationApplied` events carry the group.
- `check --deep` reports one finding for a group with a refused member.
- Members must be next to each other, and an operation field the plan format does not define is refused,
  so a misspelt `group` cannot silently run ungrouped.

Plans without groups behave as before: one check at the end of the run, edits left on disk if it fails.
