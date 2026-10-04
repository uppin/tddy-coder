# 2026-10-04 — Restructuring plan dialog in the session code explorer

**Type:** Feature

`#live-plan` 13/15 — PR [#572](https://github.com/uppin/tddy-coder/pull/572), `feature/live-plan/plan-dialog`. Cross-package entry:
[2026-10-04-plan-dialog.md](../../../../docs/dev/changesets/2026-10-04-plan-dialog.md). Product entry:
[2026-10-04-plan-dialog.md](../../../../docs/ft/web/changelog/2026-10-04-plan-dialog.md).

No production code of the daemon changes: the plan calls are served by `tddy-daemon-rpc`'s
`CodeNavigationServiceImpl`, which the daemon already wires. `tests/plan_dialog_acceptance.rs` pins them
against the production `IndexDaemonRegistry` and a fake `code_index` server holding one plan's store
state, and is registered in `test_placement.rs` `BELONGS_HERE`. Docs:
[code-navigation-service.md](../code-navigation-service.md#restructure-plans).

**Tests (scoped).** `plan_dialog_acceptance` 5 passed; `code_navigation_acceptance` 7 passed.

**Code issues.** None of this package's records (`runtime.rs`) is affected.
