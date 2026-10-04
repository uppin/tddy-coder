# 2026-10-04 — Restructuring plan dialog in the session code explorer

**Type:** Feature

`#live-plan` 13/15 — PR [#572](https://github.com/uppin/tddy-coder/pull/572), `feature/live-plan/plan-dialog`. Cross-package entry:
[2026-10-04-plan-dialog.md](../../../../docs/dev/changesets/2026-10-04-plan-dialog.md). Product entry:
[2026-10-04-plan-dialog.md](../../../../docs/ft/web/changelog/2026-10-04-plan-dialog.md).

`code_navigation.proto` gains `OpenPlan`, `WatchPlan` and `RunPlan` with their requests
(`session_token`, `project_id`, `worktree_path`, `rel_path`), `PlanSnapshot`, `PlanOperation`,
`PlanOperationStatus`, `PlanRunEvent` (`operation`, `note`, `outcome`, `failure`) and
`PlanRunFailure{message, group, rolled_back}`. The TypeScript bindings are regenerated. Docs:
[code-navigation-proto.md](../code-navigation-proto.md#restructure-plans).

**Decisions.** The messages are declared here, not imported from `code_index`, because the web reads
only this package's protos; operations carry the plan's own vocabulary (`op`, `item`, `file`,
`group`) rather than the index's. `ROLLED_BACK` exists in the status enum so a client can show a group's
restored operations, but the daemon sets it only through a run's `failure` event.

**Code issues.** None of this package's records is affected.
