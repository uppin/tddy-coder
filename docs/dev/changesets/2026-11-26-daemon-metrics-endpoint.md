# 2026-11-26 — Prometheus `/metrics` on a private daemon listener

**Type:** Feature · **PR:** [#1046](https://github.com/uppin/tddy-coder/pull/1046) (`#fs-port` 2/13)
**Packages:** `tddy-metrics`, `tddy-daemon-kernel`, `tddy-daemon`, `tddy-agent-launch`, `tddy-rpc`, `tddy-session-lifecycle`

The daemon exposes Prometheus metrics on a separate, private listener configured on its own (`metrics.listen`; absent binds nothing), never on the public web port. New crate `tddy-metrics` owns the registry (`DaemonMetrics`), the axum `/metrics` server (`text/plain; version=0.0.4`, 404 elsewhere), the `process_open_fds` collector and the RPC instrumentation hook. `tddy-daemon-kernel` contributes `MetricsConfig` plus the web-port clash check that refuses startup naming both settings. Sessions, the RPC seam (`InstrumentedRpcService`) and LiveKit rooms feed the counters. The exact RPC service the web listener serves counts calls on the daemon registry when metrics are installed.

Deferred: live refresh of the `tddy_sessions_active` and `tddy_livekit_rooms_connected` gauges (no owner, invented semantics, no tests) — follow-up node.
