# 2026-10-04 — Restructuring plan dialog in the session code explorer

**Type:** Feature

`#live-plan` 13/15 — PR [#572](https://github.com/uppin/tddy-coder/pull/572), `feature/live-plan/plan-dialog`. Cross-package entry:
[2026-10-04-plan-dialog.md](../../../../docs/dev/changesets/2026-10-04-plan-dialog.md). Product entry:
[2026-10-04-plan-dialog.md](../../../../docs/ft/web/changelog/2026-10-04-plan-dialog.md).

`WorktreeCodePane` offers **Open as plan** for a previewed plan file (`isRestructurePlanFile`: a `.jsonl`
whose first line is a plan header) and opens `RestructurePlanDialog`, which lists operations with status,
group and staleness, disables Run while any operation is stale, and shows a run's progress and a failed
group's rolled-back rows. `createRestructurePlanApi` is the thin adapter over the three plan calls. Docs:
[code-navigation.md](../code-navigation.md#restructure-plans).

**Decisions.** A run's events override the watched snapshot for the rows they name, since they are the
freshest word until the store catches up. The dialog derives `rolled_back` from a failure event's
`rolled_back` ids, because the daemon's snapshots never carry it.

**Tests (scoped).** Cypress component `RestructurePlanDialog.cy.tsx` 6 passed.

**File length.** `WorktreeCodePane.tsx` 126 to 161 production lines; the new files are 167 and 157.

**Code issues.** None of this package's open records (`CreateSessionPane.tsx`, `SessionMainPane.tsx`,
`useModelRegistryFanOut`) names a file this change touched; none was changed.
