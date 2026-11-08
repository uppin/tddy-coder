# 2026-11-07 — Inspector, agent layers, code pane and SessionMainPane in `tddy-session-web` (#web-split 17/17, PR #861)

**Type:** Architecture change

Moves the session inspector and its tabs, the agent roster, tree and conversation layers, the code pane
(`src/code/`) and `SessionMainPane` into `tddy-session-web` — 60 modules, 24 specs on port doubles, and
the module docs. `tddy-web` keeps the drawer for #862.
Deviations from the plan: `SessionMainPane` came here from #860 (it composes the inspector and agent
layers); `PrStackPresenterRoomAcceptance` stays in `tddy-web`; a few drawer-read helpers
(`inspectorState`, `jailedCodebaseHalf`, `useMeterSnapshot`, `SessionDetailPane`) stay with the drawer.
At wrap: `bun.lock` regenerated (identical to the green's); the package's 16 own typecheck errors fixed;
`SessionMainPane.tsx` 504 → 504 lines (record open, moved).
Package detail: [tddy-session-web](../../packages/tddy-session-web/docs/changesets/2026-11-07-session-inspector-package.md),
[tddy-web](../../packages/tddy-web/docs/changesets/2026-11-07-session-inspector-package.md).
Product description: [session-package.md](../ft/web/session-package.md).

- **Theme scan fix (from #840):** `tddy-ui-web/src/theme.css`'s `@source "../../tddy-*-web/src"` matched nothing (a directory wildcard needs a file pattern), so classes used only in split packages were never generated; now `tddy-*-web/src/**/*.{ts,tsx}`. The bug dates from #840, so #840–#860 must not merge alone: the stack merges together.
