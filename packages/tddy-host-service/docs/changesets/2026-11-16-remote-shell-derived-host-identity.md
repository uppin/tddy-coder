# 2026-11-16 — Derived hosts name the daemon the web addresses

**Type:** Fix · `#remote-shell` 9/11, PR [#925](https://github.com/uppin/tddy-coder/pull/925)

`ListDerivedHosts` stamps each derived host's `parent_instance_id` with this daemon's routing id (`local_instance_id_for_config`) — the id the web addresses it with, which differs from the durable registry id when `daemon_instance_id_append_startup_timestamp` is set. `ListKnownHosts` fills `KnownHostEntry.routing_instance_id` for online rows: the local row carries this daemon's routing id, a peer's comes from `EligibleDaemonSource::routing_instance_id_of` (the default pairs `live_known_hosts()` with `list_eligible_daemons()` by position), and an offline row carries none. Nothing is persisted; the id is filled at answer time, so a restart cannot leave a stale one behind. See [host-service.md](../host-service.md).

Verification: `CARGO_INCREMENTAL=0 ./dev cargo test -p tddy-host-service` green (including `derived_hosts_identity_acceptance` 3/3); clippy `-D warnings` and `cargo fmt --check` clean.
