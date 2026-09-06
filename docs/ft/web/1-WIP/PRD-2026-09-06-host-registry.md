# PRD — Hosts screen: a durable registry of every host tddy has seen

**Date:** 2026-09-06
**PRD type:** New feature
**Product area:** `web` (screen) + `daemon` (registry)
**Stack:** `#hosts-screen` node 1 of 8 — the root node
**Branch:** `feature/hosts-screen/host-registry` → base `master`

## Affected features

| Document | Relationship |
|---|---|
| [`docs/ft/web/app-shell.md`](../app-shell.md) | A new routed screen renders inside `AppShell`; a `DaemonNavMenu` entry is added |
| [`docs/ft/web/url-state-routing.md`](../url-state-routing.md) | Adds the `#/hosts` path to the URL grammar |
| [`docs/ft/web/projects-screen-multi-host.md`](../projects-screen-multi-host.md) | Establishes "a host is a daemon instance"; this PRD extends that from *currently connected* to *ever seen* |
| [`docs/ft/web/host-stats-footer.md`](../host-stats-footer.md) | Existing host telemetry; consumed by node 2, not by this node |

## Summary

Give tddy-web a dedicated **Hosts** screen at `#/hosts` that lists every host tddy knows about —
**including hosts that are not currently connected** — and says, per host, whether it is online now
and when it was last seen.

Today the set of hosts is derived entirely from the live LiveKit participant roster plus the daemon
that served the page. A host that goes away simply vanishes: `resolveSelectedDaemonInstanceId`
(`packages/tddy-web/src/routing/selectedHost.ts:38-51`) only returns ids still present in the live
list, and the daemon's `CommonRoomPeerRegistry`
(`packages/tddy-daemon/src/livekit_peer_discovery.rs:267-269`) is an in-memory map replaced wholesale
from each room snapshot. Nothing anywhere records that a host ever existed.

This node makes host existence **durable** and gives it a screen.

## Background — why this is the root node

Every other column the Hosts screen will grow — live resource telemetry, git identity, `gh` status,
ssh-agent keys, remote-desktop availability — is a **fact about a host row**. Without a row to hang
them on, none of them can be rendered, and without durability the row disappears exactly when an
operator most wants to look at it (the host is unreachable and they want to know what it was).

So the registry and the screen are one vertical slice: a registry nobody can see is invisible, and a
screen with no rows is useless.

## Proposed changes

### What is changing

**Daemon — a durable host registry.**

- A new module in `tddy-daemon` records every host the daemon has learned about, keyed by
  `daemon_instance_id`, retaining `label`, `repos_base_path`, `max_attachment_bytes`, a
  `first_seen_unix_ms` and a `last_seen_unix_ms`.
- It is fed from the existing discovery path: whatever updates `CommonRoomPeerRegistry` also records
  into the registry, plus the local daemon's own entry.
- It **persists**, so the list survives a daemon restart and is shared across browsers (this is the
  explicit product decision — not `localStorage`).
- A host is never deleted implicitly by going offline. Disappearing from the room updates
  `last_seen_unix_ms` and nothing else.

**RPC — one new unary method on `ConnectionService`.**

```proto
rpc ListKnownHosts(ListKnownHostsRequest) returns (ListKnownHostsResponse);

message ListKnownHostsRequest { string session_token = 1; }

message KnownHostEntry {
  string instance_id = 1;
  string label = 2;
  bool online = 3;                  // present in the live roster right now
  int64 first_seen_unix_ms = 4;
  int64 last_seen_unix_ms = 5;
  string repos_base_path = 6;
  uint64 max_attachment_bytes = 7;
  bool is_local = 8;                // this daemon itself
}

message ListKnownHostsResponse { repeated KnownHostEntry hosts = 1; }
```

`online` is computed at call time by intersecting the registry with the live eligible-daemon source,
so it is never a stored value that can go stale.

**Web — the `#/hosts` screen.**

- `HOSTS_ROUTE = "/hosts"` + `isHostsPath` in `routing/appRoutes.ts`.
- `HostsAppPage` (data container, wraps `AppShell` with `variant="scroll"`) + `HostsScreen`
  (presentational), following the `VmsAppPage` / `VmsScreen` convention exactly.
- One branch in the `index.tsx` route dispatch chain.
- A `shell-menu-hosts` entry in `DaemonNavMenu`.
- One row per known host: label, instance id, an online/offline indicator, last-seen (relative for
  offline hosts, "now" for online ones), and repos base path.
- Rows sort online-first, then by label, so the useful ones are at the top.

### What is staying the same

- **The host directory is untouched.** `useHostDirectory`, the LiveKit and serving sources, and the
  `DaemonSelector` keep working exactly as they do. This screen reads the registry; it does not
  change how a host is *selected* or *reached*.
- **`HostConnection` / `ConnectionProvider` are untouched.** Reaching a host is a separate concern
  and this node does not open a connection per row.
- **`StreamHostStats` is untouched** — node 2 owns per-host telemetry.
- **No change to `ConnectionCapability`** ("rpc" / "media" / "presence"). That word means *what the
  wire carries*; this screen's future "does this host have `gh`" sense is deliberately named
  differently to avoid collision.

## Impact analysis

### Technical

- **Cross-host routing is free.** `ListKnownHosts` is a unary method on `ConnectionService`, so
  `rpc_served_by_peer` (`packages/tddy-daemon/src/connection_service.rs:9171-9183`) already relays it
  to a peer daemon when addressed to one. This node adds no transport work.
- **Two generated surfaces must be regenerated**: `cargo build -p tddy-service` (Rust) and
  `bun run --filter tddy-web generate` (TypeScript).
- **Persistence introduces a new on-disk artifact.** It must tolerate a corrupt or absent file by
  starting empty rather than failing the daemon — but must **not** silently swallow a write failure,
  since a registry that cannot persist is misreporting its own contract.
- `uint64` crosses to TypeScript as `bigint`.

### User

- A new nav entry and a new screen. Nothing existing moves or changes appearance.
- An operator can, for the first time, see that a host they used yesterday is currently offline —
  rather than it simply being absent with no explanation.

## Implementation plan

1. Registry module in `tddy-daemon` with its persistence, unit-tested in isolation.
2. Feed it from the existing discovery path; local entry always present.
3. Proto + regeneration (both languages).
4. `ListKnownHosts` handler, computing `online` against the live source.
5. Route, nav entry, `HostsAppPage` / `HostsScreen`, row rendering.
6. Cypress component acceptance tests against an in-memory backend.

## Acceptance criteria

- [ ] **AC-1** `#/hosts` renders the Hosts screen inside `AppShell`, reachable from the hamburger menu.
- [ ] **AC-2** A host currently in the live roster renders as **online**.
- [ ] **AC-3** A host in the registry but absent from the live roster renders as **offline**, with its
      last-seen time — it is **not** omitted.
- [ ] **AC-4** The registry survives a daemon restart: a host recorded before restart is still listed
      after it, as offline, with its original `first_seen_unix_ms`.
- [ ] **AC-5** Going offline updates `last_seen_unix_ms` and never removes the entry.
- [ ] **AC-6** The local daemon always appears, flagged `is_local`.
- [ ] **AC-7** `ListKnownHosts` rejects an invalid `session_token`.
- [ ] **AC-8** Rows sort online-first, then by label.
- [ ] **AC-9** A corrupt or missing persistence file yields an empty registry, not a daemon failure.

## Out of scope for this node

Live resource telemetry per row (node 2), memory/load/CPU-count (node 3), git and `gh` identity
(node 4), ssh-agent keys (nodes 5–6), remote-desktop availability and connect (nodes 7–8).

## Successor PRs

- `feature/hosts-screen/telemetry-fanout` — per-host live CPU + disk on each row.
