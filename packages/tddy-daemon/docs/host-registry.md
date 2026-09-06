# Host registry (tddy-daemon)

Which machines tddy knows about, as opposed to which ones are answering right now.

`CommonRoomPeerRegistry` (`livekit_peer_discovery.rs`) answers the second question: it is replaced
wholesale from each room snapshot, so a host that leaves the common room is simply gone. That is the
right model for routing — an RPC cannot be forwarded to a peer that is not there — and the wrong one
for an operator asking why the machine they used this morning is silent.

`host_registry.rs` is the durable half. It records a host on sight, stamps `last_seen` when the host
stops being visible, and **never removes an entry**. It backs the web's Hosts screen
([hosts-screen.md](../../tddy-web/docs/hosts-screen.md), feature
[docs/ft/web/hosts-screen.md](../../../docs/ft/web/hosts-screen.md)).

## Storage

| | |
|---|---|
| Directory | `host_registry_dir(tddy_data_dir)` = `<tddy_data_dir>/hosts` |
| File | `known-hosts.json` — one JSON array of `KnownHost` |
| Publication | `tddy_core::atomic_file::write_atomic_labelled` |

One function resolves the directory rather than the join being spelled out per call site: the daemon
builds the registry for the RPC surface and for the discovery path, and two directories would mean
discovery recording sightings the screen never reads. `runtime.rs` therefore builds **one**
`Arc<dyn HostRegistry>` and hands the same handle to both.

A `KnownHost` carries `instance_id`, `label`, `first_seen_unix_ms`, `last_seen_unix_ms`,
`repos_base_path` and `max_attachment_bytes`. The field names are the persisted JSON keys, so they
are a format: renaming one drops that column for every already-recorded host, which reads to an
operator as tddy having forgotten.

**Liveness is never persisted.** A stored "online" flag is wrong the moment a daemon exits without
notice, so `HostRegistry::known_hosts` takes the live roster as an argument and intersects.

The registry is published through `tddy_core::atomic_file` rather than the hand-rolled
staging-file-plus-rename used by `FileGitHubTokenStore`. That store and the two like it are excluded
from `atomic_file` because `write_atomic` carries permission bits over only from an *existing*
target, so a credential file would be created at the process umask on its first write. A host list is
not a credential — knowing which machines this daemon has seen grants nothing — so the reason for
that exclusion does not apply here.

**Read failures degrade, write failures surface.** A missing, unreadable or unparseable file reads as
an empty registry (with a warning), so the screen falls back to the live roster alone. A failed
**write** is returned to the caller: a registry that silently fails to persist is indistinguishable
from a working one until the restart that loses everything.

## Identity — the routing id and the durable id

A daemon has two ids for itself, and both are derived in `livekit_peer_discovery.rs` so no caller
recovers one by stripping a suffix off the other (`server-2` and `server-<startup ms>` are
indistinguishable to a string match, and guessing wrong renames a real host):

| Function | Id | Used by |
|---|---|---|
| `local_instance_id_for_config` | **routing** — the configured `daemon_instance_id` or hostname, plus the startup-timestamp suffix when `daemon_instance_id_append_startup_timestamp` is set | common-room identity, `classify_peer_route`, `ListEligibleDaemons` |
| `local_base_instance_id_for_config` | **durable** — the same id without the per-run suffix | the host registry, `ListKnownHosts` |

`EligibleDaemonSource` exposes both rosters: `list_eligible_daemons()` (what may be routed to) and
`live_known_hosts()` (the same hosts keyed by durable id). The default implementation of
`live_known_hosts` is the identity mapping, which is correct for every source that does not suffix.
`LocalOnlyEligibleDaemonSource::for_config` holds both rows for the one machine;
`LiveKitEligibleDaemonSource` builds the durable roster from each peer's advertised `host_id` plus
this daemon's un-suffixed id.

Keying durable state on the routing id would file every restart of one machine as a new host, and
"never delete a host" would then keep every one of those ghosts forever. It would also render the
serving daemon twice — once from the roster, once from the registry — with the local row marked
offline.

### `host_id` on the wire

A peer publishes its durable id as a **separate key alongside** its `DaemonAdvertisement`, via
`PublishedDaemonMetadata` (`#[serde(flatten)]` over the advertisement plus `host_id`). A reader that
knows nothing about the key — the web, or an older daemon — sees exactly the advertisement it always
saw. `parse_peer_daemon_json` returns a `PeerDaemon` (`advertisement` + `host_id`); metadata carrying
no `host_id` falls back to the peer's `instance_id`, which is what such a peer has always been filed
under.

**A peer that publishes no `host_id` and runs with the startup-timestamp suffix records one row per
restart.** Its durable id is its per-run routing id, and the registry never deletes, so each run
leaves an offline row behind. Publishing `host_id` — which every daemon built from this code does —
is what collapses those into one row.

## Recording

`CommonRoomPeerRegistry::apply_snapshot` is the seam: it adopts the new room membership, takes the
previous map out in the same operation (the difference against it is the only evidence a departure
ever produces), and records the transition. The write happens outside the routing-map guard.

- **One `record_snapshot` per room snapshot**, never one call per peer. The discovery loop ticks
  every 500 ms and a registry write is a whole-file republish with two `fsync`s, so a
  peer-at-a-time loop would turn a three-machine room into a dozen fsyncs a second on a runtime
  worker that also serves RPCs.
- A sighting is built from the **parsed advertisement**, so a remote host's `repos_base_path` and
  `max_attachment_bytes` are recorded rather than left blank. An advertisement spells "not
  advertised" as a missing key, so a zero cap is recorded as *not observed*, not as a cap of zero.
- A **failed write is logged and dropped** in the discovery path. Discovery's job is routing, and a
  persistence failure costs a stale row on a screen — whereas propagating it would let a full disk
  take the peer roster, and every RPC routed through it, down with it.
- `runtime.rs` records this daemon's **own** sighting at startup. The peer registry deliberately
  holds only remote participants, and a daemon with LiveKit switched off never syncs a room at all,
  so without it the one host an operator is certainly looking at would be the only one never
  recorded. `first_seen` for this machine therefore means "since tddy first ran here".

## Write policy

`record_snapshot` is the trait's **required** method; `record_sighting` and `record_departure` are
default implementations over it. `FileHostRegistry` holds the document in memory (loaded on first
access, so a store opened over a directory a previous process wrote picks that file up) and
republishes the file only when a snapshot changes something material:

- a host not yet recorded, or a departure that moves a `last_seen`;
- a changed `label`, `repos_base_path` or `max_attachment_bytes`;
- a `last_seen` that has fallen `LAST_SEEN_REFRESH_FLOOR_MS` (1 hour) behind.

A `last_seen` that merely advanced is deliberately not material — it advances every tick, and
counting it would make every tick a write. When a write does happen, every host visible in that
snapshot gets a fresh stamp for free.

The refresh floor is a **staleness bound, not a cache TTL and not a redundant write.** Without it a
host visible without interruption produces no material change, so its stamp never advances; a peer up
for a month and then gone while this daemon was itself down (no departure observed either, the
post-restart snapshot having nothing to diff against) would read "Offline, last seen a month ago"
about a machine that answered yesterday — the exact question the screen exists to answer, answered
wrongly. An hour bounds the error while still costing ~20,000× fewer writes than the tick.

The in-memory cache is advanced only **after** a write lands, so a failed write leaves memory
describing what is actually on disk and the next snapshot retries rather than believing a change it
never persisted.

## Bounds on peer-supplied input

`instance_id`, `label` and `repos_base_path` come from a peer's self-declared common-room metadata
and land in permanent storage with no deletion RPC. They are bounded at the store, the one boundary
every writer passes through:

| Input | Limit | On violation |
|---|---|---|
| `instance_id` | 128 bytes, and `[A-Za-z0-9]` plus `- _ . : @ +` | **refused** — a truncated id names a different host, so recording it would invent a machine |
| `label` | 256 bytes | truncated on a character boundary; the host is still recorded |
| `repos_base_path` | 512 bytes | dropped (recorded as not observed) — half a path points somewhere else |
| entry count | `MAX_KNOWN_HOSTS` = 512 | a **new** host is refused with a warning; **known** hosts still update |

Refusing at the ceiling rather than evicting is what keeps "never delete a host" true. One malformed
peer costs only its own row: the rest of the snapshot is still recorded.

`HostSighting` spells its optional columns as `Option` rather than empty-string sentinels, so a
roster row that knows only an id and a label cannot blank a `repos_base_path` a richer sighting
already recorded, and `Some(0)` remains available to mean a genuine zero.

## `ListKnownHosts`

The unary handler on `ConnectionService`. It authenticates `session_token` → GitHub user → OS user
like every other method, then joins:

```text
live_roster = eligible_daemon_source.live_known_hosts()   // durable ids
local       = local_host_sighting(config)                 // this daemon's own account of itself
hosts       = host_registry.known_hosts(&live_roster, &local, now_unix_ms)
```

The join lives in `HostRegistry::known_hosts`, not in the handler, so the invariants below hold for
every implementation rather than only for whichever double a test injects:

- **`online` is computed per call** by intersecting the registry with the live roster. The durable
  roster is used, not the routing one: intersecting the two id spaces would report the daemon
  serving the call as an offline stranger, beside a second row for itself.
- **A host in the roster but not yet recorded still appears.** The roster is a sighting in its own
  right, and hiding a machine that is answering right now would be the one failure the screen exists
  to prevent. The read does not write it back — the discovery path is what turns a roster into a
  durable record.
- **The serving daemon always has a row, flagged `is_local`.** It is demonstrably right here
  answering, so it can never legitimately be missing however little the file has managed to record;
  a first boot or an unwritable registry still shows the machine the operator is talking to, filled
  in from this daemon's own configuration.

`ConnectionServiceImpl::with_host_registry` substitutes the store, and `with_eligible_daemon_source`
the roster, so tests drive both halves of the join deterministically.

The lookup is a linear scan of the roster per entry. That is right at the scale this screen exists
for — a handful of machines; a deployment with hundreds would want a map.

## See also

- RPC surface: [connection-service.md](connection-service.md)
- Web: [hosts-screen.md](../../tddy-web/docs/hosts-screen.md),
  [host-directory.md](../../tddy-web/docs/host-directory.md)
- Feature: [docs/ft/web/hosts-screen.md](../../../docs/ft/web/hosts-screen.md)
- [changesets/](./changesets/)
