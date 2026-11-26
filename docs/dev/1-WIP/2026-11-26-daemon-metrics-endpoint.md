# Changeset: tddy-daemon exposes Prometheus `/metrics` on a private listener

**Date**: 2026-11-26
**Status**: 🚧 In Progress
**Type**: Feature
**Complexity**: 4 — new crate + new external dep (`prometheus-client`, operator-approved 2026-11-26) + config contract + daemon wiring + RPC instrumentation seam
**PRD**: [PRD-2026-11-26-daemon-metrics-endpoint.md](../../ft/daemon/1-WIP/PRD-2026-11-26-daemon-metrics-endpoint.md)

**Sequencing (hard gate):** do not start `/green` before `#target-reconnect` #1041 and #1042 have merged — they edit agent-launch, cli-sessions and session-lifecycle, which this streak migrates. Implementation runs as `#muse` sessions on `mantas-udoo-ros` (one session at a time, tight disk): keep every gate scoped with `./test -p <crate>` per crate, never a workspace run.

## Responsibility
- New crate `packages/tddy-metrics`: `DaemonMetrics` (a `prometheus_client::Registry` + the initial families,
  no global state), `serve_metrics(addr, metrics, shutdown)` (axum, `/metrics` only), `process_open_fds` collector,
  an RPC instrumentation hook.
- `tddy-daemon-kernel::config`: `MetricsConfig { listen: Option<SocketAddr> }` under `metrics:`, and the web-port clash check.
- `tddy-daemon`: wiring only — build `DaemonMetrics`, start the listener, hand counters to the services that
  update them (per `packages/tddy-daemon/CLAUDE.md`).

## Boundaries
- No metric beyond the initial seven families; no push gateway; no auth on the listener (private bind is the control).
- No FS-port work (the rest of the streak).
- Does not change the public web listener other than guaranteeing it serves no `/metrics`.

## Placement
**Streak:** `#fs-port` — n2 of 13
**Unit:** standalone
**Base:** `master`
**Why standalone:** depends on no node of the streak, and no node depends on it.

## Dependencies
None in the streak. Lands after `#target-reconnect` (#1041, #1042) only to keep the Muse queue serial; it shares no files with them.

## Draft PR contract
First push: `tddy-metrics` crate with `DaemonMetrics`, `serve_metrics` signatures (`// TODO(daemon-metrics): implement`),
`MetricsConfig` parsing, and failing tests below.

## Green wave
**Wave:** 1 of 2
**Greenable independently:** yes
**Concurrent with:** n1
**Blocks:** nothing

    n1 ⇒ n3, n4, n5, n6, n7, n8, n9, n10, n11, n12, n13      (n2: standalone)

## Scope
- [ ] `prometheus-client` added to workspace (via local crates proxy)
- [ ] `tddy-metrics` crate with the seven families and `/metrics` server
- [ ] `metrics:` config section + clash check + daemon.yaml examples/`docs/ft/daemon/daemon-settings.md`
- [ ] Wiring in `tddy-daemon` (sessions, RPC, LiveKit counters)

## Acceptance tests
- `packages/tddy-metrics/tests/endpoint.rs` — `metrics_listener_serves_every_initial_family`, `only_metrics_path_is_served`
- `packages/tddy-metrics/tests/rpc_metrics.rs` — `an_rpc_call_counts_its_service_method_and_code`
- `packages/tddy-daemon-kernel/tests/…` — `metrics_section_parses`, `metrics_listen_on_the_web_port_is_refused`, `no_metrics_section_means_no_listener`
- `packages/tddy-daemon/tests/…` — `web_listener_does_not_serve_metrics` (in-process daemon)

Gates: `./test -p tddy-metrics -p tddy-daemon-kernel -p tddy-daemon`.

## TODO

- [x] Record initial discovery
- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement
- [ ] /validate-changes
- [ ] /pr-wrap
