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

Technical: [`host-registry.md`](../../../packages/tddy-host-service/docs/host-registry.md).

## Re-read at `#unbundle` node 2 wrap (2026-09-10) — still open, unchanged

Node 2 ([PR #471](https://github.com/uppin/tddy-coder/pull/471)) was asked to re-read this entry
against what its deletions left behind. Nothing here is resolved:

- **Still exactly three test callers**, the same three: `add_project_to_host_acceptance.rs`,
  `set_project_default_branch_acceptance.rs`, `supervisor_spawn_delegation.rs` — all under
  `packages/tddy-daemon/tests/`.
- **The type moved crates.** `#unbundle` node 1 carried `multi_host.rs` into `tddy-host-service`,
  so the definition is now `packages/tddy-host-service/src/multi_host.rs:134` and the fix is a
  cross-crate change: delete a `pub` type in one crate, re-point three test files in another.
  Still small, still not this node's boundary.
- **Nothing node 2 deleted touched it.** The VNC service that node 2 removed had no relationship to
  the eligible-daemon roster; it was simply the larger unreachable-code question this entry's
  re-read turned up. That question is now settled and recorded separately in
  [`2026-09-10-vnc-proto-has-no-server-and-a-live-web-client.md`](2026-09-10-vnc-proto-has-no-server-and-a-live-web-client.md).

The natural owner is whichever `#unbundle` node next touches `packages/tddy-daemon/tests/` for
these three suites, or a standalone cleanup — it needs no plan.
