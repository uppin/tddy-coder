# 2026-09-06 — The Hosts screen at `#/hosts`

**Type:** Feature

Root node of the `#hosts-screen` stack ([#453](https://github.com/uppin/tddy-coder/pull/453)). See
the cross-package entry for the whole change:
[docs/dev/changesets/2026-09-06-host-registry.md](../../../../docs/dev/changesets/2026-09-06-host-registry.md).

`HOSTS_ROUTE = "/hosts"` and `isHostsPath` (exact match, no sub-paths) in `routing/appRoutes.ts`, one
branch in the `index.tsx` dispatch chain, and a `shell-menu-hosts` entry in `DaemonNavMenu` between
Projects and Models & Agents.

`components/hosts/HostsAppPage.tsx` and `HostsScreen.tsx` follow the `VmsAppPage` / `VmsScreen`
split. The page makes **one `ListKnownHosts` call per visit** against the selected daemon — registry
membership changes rarely, so a stream would buy nothing and would owe the `tx.closed()` teardown
contract in `packages/tddy-codegen/docs/server-streaming.md`. Its effect depends on the memoised
client and the session token, both of which genuinely invalidate the answer when they change, so a
reply in flight from a host just navigated away from is discarded by a `current` flag rather than the
whole re-read being suppressed by a ref guard.

`HostRow` is `{instanceId, label, online, lastSeenUnixMs, reposBasePath, isLocal}`. The wire's
`firstSeenUnixMs` is not mapped: there is no "known since" column, and an unrendered field is surface
every later change would have to keep mapping for nothing. Rows sort online-first then by label, over
a copy of the prop array. The `(local)` marker comes from the daemon's `is_local`, because every
daemon self-labels `"<id> (this daemon)"` and the label alone cannot say which one is serving the
page; its test id sits outside the `hosts-row-` namespace, which belongs to the row elements alone.

`hostRowFormat.ts` renders the relative last-seen phrase — "just now" under a minute, then
pluralised minutes, hours and days — taking `nowUnixMs` as an argument so tests pin a phrase instead
of racing the clock, clamping a not-yet-past stamp to "just now", and accepting a plain `number`
alongside the wire's `bigint`.

Acceptance specs in `cypress/component/HostsScreenAcceptance.cy.tsx`, with
`cypress/support/pages/hostsScreenPage.ts` selecting rows structurally
(`[data-testid="hosts-table"] tbody tr`) rather than by test-id prefix, which would also collect each
row's cells. `src/components/hosts` is in `package.json`'s `test:unit` directories, which is what
makes the unit tests there run in CI.

Module [hosts-screen.md](../hosts-screen.md); feature
[docs/ft/web/hosts-screen.md](../../../../docs/ft/web/hosts-screen.md).
