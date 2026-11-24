# Sessions drawer: ended sessions always fold, rows ordered by start and by stop

**Type:** Fix
**Packages:** `tddy-session-web`
**Feature:** [session-drawer.md § Active / Remaining Partition Separator](../../ft/web/session-drawer.md#active--remaining-partition-separator), [§ Row order](../../ft/web/session-drawer.md#row-order)

## Problem

1. **Ended sessions were listed bare when nothing in the project was live.** `SessionGroupBody`
   showed **More** only when both partitions were non-empty
   (`isPartitioned = activeCount > 0 && remainingCount > 0`). A project whose sessions had all
   ended, such as one with only its main and two idle orchestrators, listed every ended row
   unfolded.
2. **Active rows were not newest-first.** `groupSessionsByStack` returned `groups` (orchestrators
   with children) and `flat` (everything else), and the drawer rendered all groups before any flat
   row. A plain session started after an orchestrator therefore sat below it. Children were sorted
   oldest-first, and nesting went only one level deep: a session started by a child became a
   group parent *and* a child, so it was rendered twice.
3. **Ended rows had no recency order.** They kept the input order (`createdAt`), so the session
   that had just stopped could be anywhere under **More**.

## Change

- **Fold.** `SessionGroupBody` renders the remaining partition under the collapsed **More**
  whenever it is non-empty. The pinned main stays outside, as before.
- **One fold, recursive.** `sessionStackForest` (`agentTree.ts`, beside the inspector's
  `subagentSessionNodes`) folds a list into top-level rows. Each row carries the sessions that name
  it as `orchestratorSessionId`, recursively, in input order. Every session lands exactly once;
  self-links and cycles are survived. `groupSessionsByStack(sessions, order)` sorts and then folds,
  and `SessionStackGroupResult` is now `{ rows }`, replacing `{ groups, flat }`.
- **Order per partition.** Active rows are sorted by `sortSessionsByCreation` (newest-started
  first), so children follow their starter, newest first. Remaining rows are sorted by the new
  `sortSessionsByLastTouched`, which uses the later of `lastActivity.atUnixMs` and `updatedAt`
  (`sessionLastTouchedMs` in `utils/sessionSort.ts`); a session with neither known sorts last.
- **Rendering.** `SessionDrawerGroups.tsx` renders `rows`. A row with descendants is a
  `SessionStackGroup` with its toggle, its descendants listed depth-first at `depth` 1, 2, and so
  on. A row with no descendants is a plain item. A shared `DrawerRow` replaces three copies of the
  item props.

## Deferred

The daemon records no stop time: `is_active` is computed per listing from `LiveSessions`, and
nothing writes `.session.yaml` when a session leaves it. "Most recently stopped" is therefore only
as good as the last hook report. The operator chose a follow-up for this:
[`docs/dev/todo/2026-11-24-session-stop-time-not-recorded.md`](../todo/2026-11-24-session-stop-time-not-recorded.md).

## Tests

- `src/utils/sessionStackGroups.test.ts`, rewritten for `rows`: newest-first roots, children
  after their starter newest-first, recursive nesting, orphans, cycles, active order, remaining
  order by last touch, and an ended stack's children.
- `cypress/component/SessionsDrawerActiveSeparator.cy.tsx`: folding with nothing active, active
  row order, and row order under **More**.
- `cypress/component/SessionsDrawerProjectHomeAcceptance.cy.tsx`: main pinned with **More (2)**
  when nothing is live (the reported case).
- Specs that click an ended row in a project with nothing live now open **More** first
  (`sessionsDrawerPage.expandRemaining()`).
