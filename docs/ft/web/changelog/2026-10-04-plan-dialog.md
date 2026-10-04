# 2026-10-04 — Restructuring plan dialog in the code pane

`#live-plan` 13/15 — PR [#572](https://github.com/uppin/tddy-coder/pull/572), `feature/live-plan/plan-dialog`.

Previewing a restructure plan file in a session's code pane shows **Open as plan**. The dialog lists every
operation of the plan with its id, kind, item and file, group and status, and marks an operation *stale —
`<reason>`* when it no longer points at the code it was written against. A stale operation disables
**Run** and is named; otherwise **Run** applies the plan through the warm index and each row turns applied
as its operation lands. A transactional group that does not compile stops the run and shows its
operations rolled back. Other `.jsonl` files, such as event logs, offer no plan entry. See
[Worktree Code Pane § Restructure plans](../session-code-pane.md#restructure-plans).
