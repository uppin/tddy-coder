# Changeset: tddy-daemon exposes Prometheus `/metrics` on a private listener

**Date**: 2026-11-26
**Status**: 🚧 In Progress — green implemented (this node), gates pending
**Type**: Feature
**Complexity**: 4 — new crate + new external dep (`prometheus-client`, operator-approved 2026-11-26) + config contract + daemon wiring + RPC instrumentation seam
**PR**: [#1046](https://github.com/uppin/tddy-coder/pull/1046)
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
**Concurrent with:** n1 (#1045)
**Blocks:** nothing

    n1 ⇒ n3, n4, n5, n6, n7, n8, n9, n10, n11, n12, n13      (n2: standalone)

## Red-step state

Published surface: `tddy_metrics::{DaemonMetrics (new, session_started, set_sessions_active, rpc_completed, set_livekit_rooms_connected, encode), MetricsServer (bind, local_addr, serve), StartResult, RpcCall}`;
`tddy_daemon_kernel::config::{MetricsConfig, check_metrics_listen}` and `DaemonConfig::metrics`.
`prometheus-client` resolved at 0.23.1 (0.26 exists — green may bump if the API is better; operator approval covers the crate).

Failing (on `TODO(daemon-metrics)`): `tddy-metrics/tests/endpoint.rs` — 5 tests;
`tddy-daemon-kernel/tests/metrics_config_unit.rs` — `metrics_on_the_web_port_is_refused_naming_both_settings`,
`metrics_on_its_own_port_is_accepted`. Passing by design: the two parse tests (the config type *is* the published surface).
Not yet written (green, wiring): `tddy-daemon` in-process check that the web listener serves no `/metrics` and that
an RPC through `MultiRpcService` reaches `rpc_completed` — the daemon has no seam for it until green adds the wiring.
Baseline: build + clippy `--all-targets` clean for tddy-metrics and tddy-daemon-kernel; the kernel's full suite was not run locally.

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
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement
- [ ] /validate-changes
- [ ] /pr-wrap

## Green notes (2026-11-27, n2 green)

- `tddy-metrics`: `DaemonMetrics` owns one `Registry` + the seven families (`Info` for
  `tddy_build_info`, counter families for starts/RPC, gauge families for active sessions,
  `Gauge` for LiveKit rooms and `process_open_fds` — refreshed inside `encode()`, so a direct
  call and an HTTP scrape read a current value). `MetricsServer` serves `GET /metrics` as
  `text/plain; version=0.0.4`, 404 elsewhere.
- `check_metrics_listen`: port equality refuses, naming both settings; no section (or no
  `web_port`) is accepted. `main.rs` gates startup on it; `dev.daemon.yaml` carries a commented
  example; `daemon-settings.md` documents the section (settings saves carry it through untouched).
- RPC seam: `tddy_rpc::InstrumentedRpcService` (the port's crate, per `tddy-daemon/CLAUDE.md`)
  records dispatch latency + lowercase code (`ok`, `not_found`, …); `server::rpc_service_for`
  installs it when the runtime built metrics, and the daemon test drives that exact seam.
- Sessions: `start_session_core` (the documented choke point) counts `Started`/`Failed` via an
  optional handle threaded host → `LaunchSessions` with `with_metrics` (no `new` signature
  churn across its 13 call sites).
- Deferred (no owner, invented semantics, no test coverage): live refresh of the
  `tddy_sessions_active` and `tddy_livekit_rooms_connected` gauges. Their setter APIs ship tested
  (`endpoint.rs`); wiring them needs roster/room query APIs that do not exist (`CliSessionManager`
  exposes no counts; room state is spread across the supervisor/session rooms). Follow-up node.
