# 2026-09-06 — A durable record of every host this daemon has seen

**Type:** Feature

Root node of the `#hosts-screen` stack ([#453](https://github.com/uppin/tddy-coder/pull/453)). See
the cross-package entry for the whole change:
[docs/dev/changesets/2026-09-06-host-registry.md](../../../../docs/dev/changesets/2026-09-06-host-registry.md).

`host_registry.rs`: `KnownHost`, `HostSighting`, `KnownHostView`, the `HostRegistry` trait and a
`FileHostRegistry` keeping one JSON array at `<tddy_data_dir>/hosts/known-hosts.json`, published
through `tddy_core::atomic_file::write_atomic_labelled`. `record_snapshot` is the required trait
method — `record_sighting` and `record_departure` are default impls over it — because the discovery
loop produces snapshots, and recording a three-peer room one peer at a time would be three
read-modify-write-fsync cycles per observation on a worker that also serves RPC. The document is
held in memory and republished only when a snapshot changes something material; a `last_seen` that
merely advanced is not material, and `LAST_SEEN_REFRESH_FLOOR_MS` (1 hour) is what stops that
suppression letting a stamp age without limit. The cache advances only after a write lands, so a
failed write leaves memory describing the disk and the next snapshot retries. Reads tolerate a
missing or unparseable file by starting empty; writes return their failure.

Peer-supplied strings are bounded at the store, the one boundary every writer passes through: an id
over 128 bytes or outside `[A-Za-z0-9-_.:@+]` is **refused** (a truncated id names a different
host), a label over 256 bytes is truncated, a `repos_base_path` over 512 bytes is dropped, and
`MAX_KNOWN_HOSTS = 512` refuses a new host while still updating known ones — eviction would
contradict "never delete a host".

**The routing id and the durable id are now derived separately**, in one place each:
`local_instance_id_for_config` carries the startup-timestamp suffix and is what the common room and
`classify_peer_route` answer to; `local_base_instance_id_for_config` drops it and is what anything
outliving the process keys on. `EligibleDaemonSource::live_known_hosts()` is the roster in the
second id space (default impl: the identity mapping), and `LocalOnlyEligibleDaemonSource` now holds
both rows for the one machine. Peers publish their durable id as a separate wire key beside the
advertisement (`PublishedDaemonMetadata`, `#[serde(flatten)]`), so an older reader sees exactly the
advertisement it always saw; `parse_peer_daemon_json` returns a `PeerDaemon` and falls back to
`instance_id` when no `host_id` is published.

`CommonRoomPeerRegistry::apply_snapshot` is the recording seam, split out of `sync_from_room` so the
snapshot diff is exercisable without a live `Room`. Sightings are built from the parsed
advertisement, so a remote host's `repos_base_path` and attachment cap are recorded rather than left
blank, with zero read as "not advertised". The write happens outside the routing-map guard, and a
failure there is logged and dropped: discovery's job is routing, and a full disk must not take the
peer roster down with it. `runtime.rs` builds one registry for the whole daemon — the RPC handler
and discovery share the handle — and records this daemon's own sighting at startup, since the peer
registry holds only remote participants and a daemon with LiveKit off never syncs a room.

`ListKnownHosts` on `ConnectionService`: authenticated like every other handler, then one join in
`HostRegistry::known_hosts` — `online` computed against the **durable** roster, a live-but-unrecorded
host still listed (a roster row is a sighting), and the serving daemon guaranteed a row flagged
`is_local`. `with_host_registry` and `with_eligible_daemon_source` inject both halves for tests.
`ListEligibleDaemons` now compares `is_local` against the routing id, so it holds for a daemon
carrying a configured instance id or the startup suffix.

Module [host-registry.md](../host-registry.md); RPC
[connection-service.md](../connection-service.md).
