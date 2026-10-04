# 2026-10-04 — Restructuring plan dialog in the session code explorer

**Type:** Feature

`#live-plan` 13/15 — PR [#572](https://github.com/uppin/tddy-coder/pull/572), `feature/live-plan/plan-dialog`. Cross-package entry:
[2026-10-04-plan-dialog.md](../../../../docs/dev/changesets/2026-10-04-plan-dialog.md). Product entry:
[2026-10-04-plan-dialog.md](../../../../docs/ft/web/changelog/2026-10-04-plan-dialog.md).

`CodeNavigationServiceImpl` serves `OpenPlan`, `WatchPlan` and `RunPlan`, each through
`authorise_and_connect`. New `code_navigation/plan.rs`: `read_plan_rows` (listed-file read, `Plan::parse`,
store-assigned ids), `open_snapshot` / `current_snapshot` / `snapshot_of` (journal counts laid over the
rows in plan order, stale reasons folded in), `follow` (the polling `WatchPlan` loop), `relay_run` /
`run_event` (the `Apply` stream as plan run events) and `run_failure` (a `GroupDoesNotCompile` refusal
becomes a failure naming the group, its rows rolled back). `tddy-code-restructuring` becomes a dependency.
Docs: [architecture.md](../architecture.md#restructure-plans).

**Decisions.** The plan file is read once per call and its rows reused by the watch and run loops. The
loops live in `plan.rs` (`follow`, `relay_run`) so the three trait methods stay short. The group is read
back from the refusal's wording because the index puts no group outcome on the wire.

**Deferred.** `snapshot_of` is approximate after a partial run, and `run_plan` sends `resume: false`;
see the cross-package entry.

**Code issues.** None of this package's open records names a file this change touched; none was changed.
