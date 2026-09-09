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

Its directory is where the daemon keeps everything else scoped to a host rather than to a session —
the host keypair, and the [desktop targets](#host-scoped-desktop-targets) a machine's remote
desktops are opened from.

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

## Host-scoped desktop targets

`host_desktop_targets.rs` keeps the remote desktops attached to a **host** rather than to a session,
in `host-desktop-targets.json` in the same `host_registry_dir(tddy_data_dir)` the known-hosts file
lives in. That is where it belongs: a desktop is a property of a machine, and the host keypair a
desktop password is encrypted under is already resolved from this directory, so one host has one
directory holding everything scoped to it.

| | |
|---|---|
| File | `host-desktop-targets.json` — `{ hosts: { <daemon_instance_id>: [HostDesktopTarget] } }` |
| A target | `target_id`, `label`, `host`, `port`, `protocol`, `username` |
| Publication | `tddy_core::atomic_file`, under a mutex held across each read-modify-write |

The map is a `BTreeMap` so the file an operator opens is ordered the same way between writes rather
than reshuffled by hash order, and the wrapper object exists so a later field — a format version,
say — does not make every already-written file unreadable. `protocol` holds
`screen_sharing.proto`'s `Protocol` discriminants rather than a second enumeration of them, for the
same reason the tooling probe's block does: two enums meaning one thing are two enums that drift.

**This store is not a credential store**, which is what keeps it out of the exclusion that applies
to the daemon's secret files: a label, a host, a port and a username grant nothing, so plain
`write_atomic` is right here and the hand-rolled staging-file pattern is not copied by reflex.

**A file that exists and does not parse is an error, never an empty set.** Collapsing the two would
show an operator a machine with no desktops when it has several, make every start against one report
"no such target", and — because the callers that go on to write would replace a damaged file with a
file holding one target — lose every other host's desktops with no way back. `list` is therefore
fallible, and the read-modify-write is serialised so two concurrent attaches do not drop one
another's target.

The two scopes never see each other. A host-scoped target is invisible to
`screen_sharing_vault`'s session store and a session-scoped one is invisible here, so deleting a
session cannot delete a machine's desktop.

### Starting and stopping a host's desktop

`ScreenSharingService` carries the host-scoped half of its surface —
`ListHostTargets`, `AddHostTarget`, `StartHostStream`, `StopHostStream` — addressed by
`daemon_instance_id` and `target_id` where the session-scoped calls take a `session_id`. They call
the same spawn path the session calls do; host scope is an addressing and storage change, and the
bridges, the LiveKit republishing and the browser overlay are reused unchanged.

- **Every one of them requires the operator's session token**, and the daemon resolves the GitHub
  user from it. A daemon that was built without host scope answers all four with
  `FAILED_PRECONDITION` rather than a plausible empty result.
- **The room is the daemon's configured `livekit.common_room`.** A session has a room in its
  metadata; a host has none, and the common room is the one a browser on the Hosts screen already
  holds a token for. A daemon with no LiveKit configuration is a `FAILED_PRECONDITION`, not a start
  that returns coordinates nothing can join. This is deliberately not gated on `livekit.enabled`,
  which governs whether *this* daemon joins a room.
- **The bridge identity is `screenshare-host-{instance_id}-{target_id}`.** Every host's bridge lands
  in that one common room, so the host id has to be part of the identity. The track name is the same
  `screenshare:<target_id>` the session path publishes.
- **Spawn failure is reported**, unlike the deliberately silent session-scoped spawn whose callers
  depend on getting coordinates back regardless. These coordinates tell a browser to mount an
  overlay, and an operator who has just typed a secret must not be left watching a room no bridge
  joined.
- **A reopen terminates the bridge the previous open left**, keyed by host and target, so a second
  start cannot orphan the first process.
- `StopHostStream` checks host scope for the same reason: `ok: true` from a daemon that could not
  have been holding a bridge open tells the browser to tear down an overlay over a process still
  running.

### The desktop password is prompted, never stored

The session-scoped path keeps its credential in an encrypted vault. A host desktop's password is
**not stored anywhere**:

1. `StartHostStream` settles everything that can fail without a secret first — the target, the room,
   the bridge binary, the protocol — so nobody is asked to type a password for a stream that could
   not have started.
2. It then raises a `DESKTOP_PASSWORD` prompt on the host prompt channel, stamped with the GitHub
   user resolved from the caller's session token. That stamping is what makes it *this* operator's
   question: it is shown to no other browser and no other browser can spend its one answer.
3. The browser encrypts its answer under the public key the host publishes with the prompt. The
   daemon decrypts it with the host keypair — the same one read from this directory — hands the
   plaintext to the bridge on the bridge's **stdin**, and drops it when the call returns. No argv,
   no file, no daemon state.

**Every start asks.** Nothing records whether a desktop wants a password, and a daemon that guessed
would either skip the question for one that needs it or refuse one that does not; an operator
answering with nothing is how a password-less desktop is opened.

**The wait is exactly the prompt's own expiry.** An operator who walks away releases the call the
moment the question stops being answerable — never later and never never. An unanswered prompt fails
the start with `DeadlineExceeded` and **spawns no bridge**: a bridge started without the password it
needed authenticates to nothing and would leave a process running for a stream that can never carry
a frame.

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
- Desktop reachability on the same row: [host-tooling-probe.md](host-tooling-probe.md#remote-desktop)
- Feature: [docs/ft/web/screen-sharing-sessions.md](../../../docs/ft/web/screen-sharing-sessions.md)
  — both scopes of the remote-desktop feature
- Web: [hosts-screen.md](../../tddy-web/docs/hosts-screen.md),
  [host-directory.md](../../tddy-web/docs/host-directory.md)
- Feature: [docs/ft/web/hosts-screen.md](../../../docs/ft/web/hosts-screen.md)
- [changesets/](./changesets/)
