# 2026-09-06 — `host_id` should become a field of `DaemonAdvertisement`

**Category:** Future enhancement
**Source:** host registry, #453 (`#hosts-screen` 1/8)

A peer publishes its **durable** host id so the registry can file it under one row across restarts,
rather than under the per-run instance id it routes by. That key currently rides *beside* the
advertisement via `#[serde(flatten)]` instead of being a field of `DaemonAdvertisement`, which keeps
the published metadata byte-identical for peers that never heard of it.

Folding it in is about two lines. It is blocked only by the round-trip equality assertion in
`packages/tddy-daemon/tests/daemon_advertisement_attachment_cap.rs`, which constructs the struct as a
literal and compares the parsed value to it — adding a field breaks compilation there, and that file
belongs to another change.

Until a peer publishes `host_id`, it is filed under its routing id and a peer running with
`daemon_instance_id_append_startup_timestamp` leaves one permanent offline row per restart, since the
registry never deletes.

Technical: [`host-registry.md`](../../../packages/tddy-host-service/docs/host-registry.md) § `host_id` on the wire.
