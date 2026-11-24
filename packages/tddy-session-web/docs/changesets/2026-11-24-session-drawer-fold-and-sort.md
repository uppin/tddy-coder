# 2026-11-24 — Sessions drawer: ended sessions always fold; rows ordered by start and by stop
**Type:** Fix

**Fold.** `SessionGroupBody` (`SessionDrawerGroups.tsx`) renders the remaining partition under the
collapsed **More** whenever it is non-empty. It used to require an active row as well, and the
pinned main never counts as a row. So a live main above grey rows, or a project with only ended
sessions, listed the ended rows bare.

**One recursive fold.** `sessionStackForest` (`agentTree.ts`, beside `subagentSessionNodes`) folds a
list into top-level rows, each carrying the sessions that name it as `orchestratorSessionId`,
recursively. Every session lands exactly once, and self-links and orchestrator cycles are survived.

The old `groupSessionsByStack` result was `{ groups, flat }`, rendered groups before flat rows,
sorted children oldest-first and nested only one level. It is now
`groupSessionsByStack(sessions, order)` → `{ rows }`. That also fixes a middle session, both a
child and a starter, being rendered twice.

**Order.** The active partition sorts with `sortSessionsByCreation` (newest-started first). The
remaining partition sorts with the new `sortSessionsByLastTouched`. It uses `sessionLastTouchedMs`,
the later of `lastActivity.atUnixMs` and `updatedAt`, and a session with neither known sorts last.
The order applies to roots and to each set of siblings, and a starter keeps its own place.

**Rendering.** A row with descendants is a `SessionStackGroup` (toggle kept), with its descendants
depth-first at `depth` 1, 2, and so on. A row with none is a plain item. A shared `DrawerRow` replaces
three copies of the item props.

**More** (`SessionDrawerSeparator`) now carries `aria-expanded`. The Cypress page objects (here and in
`tddy-web`) gained `selectEndedSession(id)`, which opens the row's **More** unless it is already open,
then clicks the row. About 25 session-web specs and 5 tddy-web specs that clicked a bare ended row now
use it; `expandRemaining()` is a toggle and would re-close the fold on a second ended row.

Tests: `sessionStackGroups.test.ts` rewritten for `rows` (14), Cypress
`SessionsDrawerActiveSeparator` +3, `SessionsDrawerProjectHomeAcceptance` +2 (main live / main ended).

Deferred: the daemon does not stamp a stop time, so "most recently stopped" is only as good as the
last hook report — `docs/dev/todo/2026-11-24-session-stop-time-not-recorded.md`.

Feature: [session-drawer.md § Row order](../../../../docs/ft/web/session-drawer.md#row-order).
