# 2026-11-26 — Prometheus `/metrics` on a private listener

**Type:** Feature · PR [#1046](https://github.com/uppin/tddy-coder/pull/1046) (`#fs-port` 2/13)

`MetricsConfig { listen: Option<SocketAddr> }` under `metrics:`, plus the web-port clash check that refuses startup naming both settings. Covered by config parse and clash unit tests. Cross-package entry:
[`docs/dev/changesets/2026-11-26-daemon-metrics-endpoint.md](../../../../docs/dev/changesets/2026-11-26-daemon-metrics-endpoint.md).
