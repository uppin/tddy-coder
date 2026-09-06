# Hosts screen

Every host tddy has a record of, reachable or not.

The [host directory](host-directory.md) answers "who can this page talk to right now", and every
host-selection surface reads it. The Hosts screen answers a different question: which machines does
the daemon *remember*. Its reason to exist is the **offline row** — a host that has left the common
room disappears from every other surface in tddy, which is exactly the moment an operator wants to
look at it.

The rows come from the daemon's durable registry
([host-registry.md](../../tddy-daemon/docs/host-registry.md)), not from the directory, so an offline
host is listed with its last-seen time rather than omitted.

## Route and entry point

| | |
|---|---|
| Route | `HOSTS_ROUTE = "/hosts"` and `isHostsPath` in `src/routing/appRoutes.ts` — an exact match, no sub-paths |
| Dispatch | one branch in the `src/index.tsx` route chain |
| Nav | `shell-menu-hosts` in `DaemonNavMenu`, between **Projects** and **Models & Agents** |

## Components

`HostsAppPage` / `HostsScreen` follow the `VmsAppPage` / `VmsScreen` split: the page owns the data,
the screen renders what it is given.

**`HostsAppPage`** wraps `AppShell` (`title="Hosts"`, the default `scroll` variant,
`data-testid="hosts-app-page"`) and makes **one `ListKnownHosts` call per visit** against the
selected daemon's client. Registry membership changes rarely, so a stream would buy nothing and would
owe the `tx.closed()` teardown contract in `packages/tddy-codegen/docs/server-streaming.md`.

The effect depends on the memoised client and the session token, both stable while the screen is
looking at the same host under the same session, so it fires once on mount. It fires again only when
the selected host changes or the token is renewed — each of which genuinely invalidates the answer,
because the registry belongs to the daemon that was asked. A ref guard would suppress exactly those
legitimate re-reads, so the in-flight reply from a host just navigated away from is discarded by a
`current` flag instead. A failed call renders `hosts-error` above the table and leaves the previous
rows in place.

**`HostsScreen`** is presentational. `HostRow` is `{instanceId, label, online, lastSeenUnixMs,
reposBasePath, isLocal}` — the wire's `firstSeenUnixMs` is deliberately not mapped, because the
screen has no "known since" column and an unrendered field is surface every later change would have
to keep mapping for nothing.

## Rows

One `<tr>` per host, in a `hosts-table`, columns left to right:

| Column | Content |
|---|---|
| Host | `label`, plus a `(local)` marker when `isLocal` |
| Status | `Online` / `Offline` (offline is muted) |
| Last seen | `formatLastSeen(lastSeenUnixMs, nowUnixMs)` |
| Instance ID | `instanceId`, monospaced |
| Repos base path | `reposBasePath`, monospaced |

An empty list renders `hosts-empty` ("No hosts recorded yet.") instead of the table.

**Sort: online first, then by label** (`byLivenessThenLabel`, over a copy — the prop array is not
mutated). Ordering by liveness is the point: the hosts an operator can act on right now belong at
the top, and the rest stay listed rather than disappearing.

**The `(local)` marker can only come from the daemon.** Every daemon self-labels
`"<id> (this daemon)"` in its own advertisement, so in a list of hosts the label alone cannot say
which one is serving this page; `is_local` is the daemon's answer. Its test id
(`hosts-local-marker-<instanceId>`) sits **outside** the `hosts-row-` namespace on purpose: that
prefix belongs to the row elements, and a marker named under it would have to be excluded by hand
from any prefix match over rows.

`formatLastSeen` (`hostRowFormat.ts`) renders a short relative phrase — "just now" under a minute,
then whole minutes, hours and days, pluralised ("1 minute ago", never "1 minutes ago"). `nowUnixMs`
is passed in rather than read from the clock so a test pins a phrase instead of racing real time. A
stamp that is not in the past reads "just now": the daemon's clock and the browser's need not agree
to the second, and a host last seen "in 4 seconds" is noise. It accepts the wire's `bigint` as well
as a plain `number`; millisecond stamps are far below 2^53, so widening loses nothing.

## Testing

`cypress/component/HostsScreenAcceptance.cy.tsx` mounts
`mountWithRpc(withSelectedDaemon(<HostsAppPage />), backend)` against the in-memory backend, and
covers online, offline-with-last-seen, the sort, the local marker in both its positive and negative
case, reaching the screen from the nav menu, and the one-RPC-per-visit boundary.

The page object `cypress/support/pages/hostsScreenPage.ts` selects rows **structurally**
(`[data-testid="hosts-table"] tbody tr`) rather than by a `hosts-row-` prefix. A prefix match over
that namespace also collects each row's own cells, and a `:not()` denylist patching around that would
silently over-match the moment a column is added.

`src/components/hosts/hostRowFormat.test.ts` pins the phrasing against a frozen clock;
`src/routing/appRoutes.test.ts` pins `isHostsPath` (positive, root, a sibling route, a sub-path).
`src/components/hosts` is listed in `package.json`'s `test:unit` directories, which is what makes
those unit tests run in CI.

## See also

- Daemon: [host-registry.md](../../tddy-daemon/docs/host-registry.md),
  [connection-service.md](../../tddy-daemon/docs/connection-service.md)
- Web: [host-directory.md](host-directory.md), [host-connections.md](host-connections.md)
- Feature: [docs/ft/web/hosts-screen.md](../../../docs/ft/web/hosts-screen.md)
