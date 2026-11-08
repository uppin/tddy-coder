# 2026-11-07 — Sessions drawer and project home in `tddy-session-web` (#web-split 18/20, PR #862)

**Type:** Architecture change

Moves the sessions drawer, project home and the status strip into `tddy-session-web`, completing the
session package: `tddy-web` keeps no session component. The drawer-driven specs earlier nodes left
behind run from the package on port doubles (136 specs); a `SessionTrafficProvider` seam carries the
app's traffic meters. Wiring specs (real LiveKit, real token HTTP, the app shell) stay in `tddy-web`.
At wrap: the package's `src/` typechecks clean; the 181 fixture-typing errors in the moved specs are
deferred to `docs/dev/todo/2026-11-07-session-web-specs-do-not-typecheck.md` (the package stays on the
`Web lint` exclusions, now for that record).
Package detail: [tddy-session-web](../../packages/tddy-session-web/docs/changesets/2026-11-07-session-drawer-package.md),
[tddy-web](../../packages/tddy-web/docs/changesets/2026-11-07-session-drawer-package.md).
Product description: [session-package.md](../ft/web/session-package.md).
