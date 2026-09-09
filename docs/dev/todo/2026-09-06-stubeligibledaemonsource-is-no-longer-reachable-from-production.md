# 2026-09-06 — `StubEligibleDaemonSource` is no longer reachable from production code

**Category:** Future enhancement
**Source:** host registry, #453 (`#hosts-screen` 1/8)

`ConnectionServiceImpl::new` now builds its no-LiveKit roster from
`LocalOnlyEligibleDaemonSource::for_config`, so `StubEligibleDaemonSource` has no production caller
left. Three files under `packages/tddy-daemon/tests/` still use it.

It is not merely redundant, it is wrong: it lists the local daemon under its **hostname**, ignoring a
configured `daemon_instance_id` and the startup-timestamp suffix. A test built on it therefore
describes a daemon no real deployment runs — which is exactly the mismatch that made the serving
daemon appear twice, and read Offline, on the Hosts screen before #453 split routing identity from
durable identity.

Switch those three callers to `LocalOnlyEligibleDaemonSource::for_config` and delete the type. Left
out of #453 because `packages/tddy-daemon/tests/` is outside that node's boundary.

Technical: [`host-registry.md`](../../../packages/tddy-daemon/docs/host-registry.md).
