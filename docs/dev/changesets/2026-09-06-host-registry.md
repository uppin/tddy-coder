# 2026-09-06 — Host existence is durable, and it has a screen

**Type:** Feature

Root node of the `#hosts-screen` stack ([#453](https://github.com/uppin/tddy-coder/pull/453)), base
`master`.

Until now, the set of hosts *was* the set of hosts answering: the live LiveKit participant roster
plus the daemon serving the page. A host that left the common room vanished from tddy entirely, with
no record it had ever existed — which is the correct behaviour for choosing where to run a session,
and removes the row at exactly the moment an operator wants it, when the machine is unreachable and
they want to know what it was.

**The daemon now keeps a durable registry of every host it has seen**, one JSON array at
`<tddy_data_dir>/hosts/known-hosts.json`, published through `tddy_core::atomic_file`. It records a
host on sight, stamps `last_seen` on departure, and never removes an entry. `first_seen` is never
re-stamped. Reads tolerate a missing or corrupt file by starting empty — the screen then shows the
live roster alone; a **write** failure is returned rather than swallowed, because a registry that
silently fails to persist is indistinguishable from a working one until the restart that loses
everything. Persistence lives in the daemon rather than the browser deliberately: the list must
survive a reload and be the same list in every browser.

**Liveness is computed, never stored.** A persisted online flag is wrong the moment a daemon exits
without notice, so `ListKnownHosts` intersects the registry with the live eligible-daemon roster per
call. The whole join lives in `HostRegistry::known_hosts` — including the guarantee that the serving
daemon always has a row — so a live-but-unrecorded host is still listed (a roster row is a sighting
in its own right) and the invariants hold for every implementation rather than for whichever double
a test injects.

**One machine is one row, across its restarts.** This is the structural change the rest rests on: the
daemon's **routing** id (`local_instance_id_for_config`, carrying the startup-timestamp suffix that
keeps LiveKit identities unique per run) and its **durable** id (`local_base_instance_id_for_config`)
are derived separately, in one place each, and never recovered from one another by stripping a suffix
— `server-2` and `server-<startup ms>` are indistinguishable to a string match, and guessing wrong
renames a real host. `EligibleDaemonSource::live_known_hosts()` is the roster in the durable id
space. Peers publish their durable `host_id` as a separate wire key beside the advertisement, so an
older reader sees exactly the advertisement it always saw and a peer that publishes none falls back
to its instance id. Keying the registry on the routing id instead would have filed every restart as a
new host, and "never delete" would have kept every one of those ghosts forever.

The recording hook rides the existing discovery path: `CommonRoomPeerRegistry::apply_snapshot`
records **one snapshot per room tick**, not one call per peer — the loop ticks every 500 ms and a
write is a whole-file republish with two fsyncs. The store then decides whether the snapshot changed
anything material, and an unchanged tick touches no disk at all; `LAST_SEEN_REFRESH_FLOOR_MS` (1
hour) is a staleness bound on that suppression, not an optimisation, because a host visible without
interruption otherwise never gets a fresh stamp and reads "last seen a month ago" the moment it goes
away. Sightings are built from the parsed advertisement, so a remote host's repos base path and
attachment cap are recorded rather than blank. A persistence failure in this path is logged and
dropped — discovery's job is routing, and a full disk must not take the peer roster down with it.

Peer-supplied strings reach permanent storage that has no deletion RPC, so they are bounded at the
store: an id over 128 bytes or outside a restricted charset is **refused** (a truncated id names a
different host), a label is truncated, an over-long path is dropped (half a path points somewhere
else), and a 512-entry ceiling refuses new hosts while still updating known ones — eviction would
contradict never deleting.

`connection.proto` gains `ListKnownHosts` with `ListKnownHostsRequest` / `KnownHostEntry` /
`ListKnownHostsResponse`, regenerated in both languages. It is unary, so `rpc_served_by_peer` already
relays it to a peer daemon at no transport cost, and a stream would have added the `tx.closed()`
teardown obligation for membership that changes rarely.

**`#/hosts`** renders one row per host — label, `(local)` marker, Online/Offline, a relative
last-seen phrase, instance id, repos base path — sorted online-first then by label, from a single
`ListKnownHosts` call per visit. `HostsAppPage` / `HostsScreen` follow the `VmsAppPage` / `VmsScreen`
split, and a `shell-menu-hosts` entry sits between Projects and Models & Agents.

The host directory, the daemon selector, `HostConnection` and `StreamHostStats` are untouched: this
screen reads what the daemon remembers and opens no connection per row. The columns that hang off
these rows — live telemetry, host resources, host identity, agent keys, remote-desktop probe and
connect — arrive with [#454](https://github.com/uppin/tddy-coder/pull/454) through
[#460](https://github.com/uppin/tddy-coder/pull/460).

Follow-ups this node leaves open are recorded in [`docs/dev/todo/`](../todo/), one file each:
retiring [`StubEligibleDaemonSource`](../todo/2026-09-06-stubeligibledaemonsource-is-no-longer-reachable-from-production.md)
from the three test files still using it, folding
[`host_id` into `DaemonAdvertisement`](../todo/2026-09-06-host-id-should-become-a-field-of-daemonadvertisement.md),
and covering the [`index.tsx` route-dispatch join](../todo/2026-09-06-route-dispatch-joins-in-index-tsx-are-uncovered.md). `ListKnownHosts`
also adds a handler, a field and two builder overrides to `connection_service.rs`, whose size is its
own recorded TODO and whose split needs its own PR.

Feature [hosts-screen.md](../../ft/web/hosts-screen.md),
[app-shell.md](../../ft/web/app-shell.md),
[url-state-routing.md](../../ft/web/url-state-routing.md); technical
[host-registry.md](../../../packages/tddy-daemon/docs/host-registry.md),
[hosts-screen.md](../../../packages/tddy-web/docs/hosts-screen.md).
(tddy-service, tddy-daemon, tddy-web)
