# 2026-10-04 — Restructuring plan dialog in the session code explorer

**Type:** Feature

`#live-plan` 13/15 — PR [#572](https://github.com/uppin/tddy-coder/pull/572), `feature/live-plan/plan-dialog`. Product entry:
[2026-10-04-plan-dialog.md](../../ft/web/changelog/2026-10-04-plan-dialog.md).

| Package | Entry |
|---|---|
| `tddy-service` | [plan-dialog](../../../packages/tddy-service/docs/changesets/2026-10-04-plan-dialog.md) |
| `tddy-daemon-rpc` | [plan-dialog](../../../packages/tddy-daemon-rpc/docs/changesets/2026-10-04-plan-dialog.md) |
| `tddy-daemon` | [plan-dialog](../../../packages/tddy-daemon/docs/changesets/2026-10-04-plan-dialog.md) |
| `tddy-web` | [plan-dialog](../../../packages/tddy-web/docs/changesets/2026-10-04-plan-dialog.md) |

**What it does.** Previewing a restructure plan file in the web code pane offers **Open as plan**. The
dialog lists the plan's operations with id, kind, item and file, group, status and staleness, and **Run**
applies the plan through the warm index with per-operation progress. A stale operation disables Run and
is named; a transactional group that does not compile shows its operations rolled back.
`code_navigation.CodeNavigationService` gains `OpenPlan`, `WatchPlan` and `RunPlan`, served by
`tddy-daemon-rpc` through the authorisation and forwarding the navigation calls already use, onto the
index daemon's `LoadPlans`, `PlanStatus` and `Apply`.

**Decisions.**

- The index daemon's plan RPCs answer less than the dialog shows (an operation count, journal counts), so
  the daemon reads the plan file itself — through `WorktreeService`'s listed-file read and
  `tddy-code-restructuring`'s `Plan::parse`, a new dependency of `tddy-daemon-rpc` — and folds in the
  store's stale reasons. No index-daemon RPC changed.
- No group outcome is on the index's wire, so a group failure is recognised from
  `RestructureError::GroupDoesNotCompile`'s `FAILED_PRECONDITION` wording and every operation of the named
  group is reported rolled back.
- `WatchPlan` polls `PlanStatus` once a second and sends a snapshot only when it differs; the store has
  no change feed.

**Deferred.** Per-operation status is approximate after a partial run, because `PlanStatus` returns
journal counts (backlog entry `2026-10-04-plan-dialog-status-is-approximate-after-a-partial-run`); and
`RunPlan` always applies from the first operation with no resume, so re-running after a partial run is
likely refused (`2026-10-04-plan-dialog-run-never-resumes`). Neither was in the backlog before; both are
new entries.

**Tests (scoped).** `tddy-daemon` `plan_dialog_acceptance` 5 passed (it also covers the group
rolled-back relay); `tddy-daemon-rpc` `--lib code_navigation` 6 passed; `code_navigation_acceptance` 7
passed; Cypress component `RestructurePlanDialog.cy.tsx` 6 passed. `clippy -p tddy-daemon-rpc -p
tddy-daemon --tests -- -D warnings` clean. The rest is CI's.

**Code issues.** None of the open records in `tddy-daemon-rpc`, `tddy-daemon`, `tddy-service` or
`tddy-web` names a file this change touched, so none was re-measured or changed and none claimed this PR.
**File length** (production lines): `code_navigation.rs` 300 to 406, `plan.rs` 0 to 292,
`RestructurePlanDialog.tsx` 0 to 167, `restructurePlanApi.ts` 0 to 157, `WorktreeCodePane.tsx` 126 to
161; none reaches 500.
