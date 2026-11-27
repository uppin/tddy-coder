# 2026-11-26 — Prometheus `/metrics` on a private listener

**Type:** Feature · PR [#1046](https://github.com/uppin/tddy-coder/pull/1046) (`#fs-port` 2/13)

New crate: a registry with the initial families, an axum `/metrics` server, a `process_open_fds` collector, and an RPC instrumentation hook. Covered by endpoint and rpc-metrics acceptance suites. Cross-package entry:
[`docs/dev/changesets/2026-11-26-daemon-metrics-endpoint.md](../../../../docs/dev/changesets/2026-11-26-daemon-metrics-endpoint.md).
