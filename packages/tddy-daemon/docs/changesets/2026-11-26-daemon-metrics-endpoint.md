# 2026-11-26 — Prometheus `/metrics` on a private listener

**Type:** Feature · PR [#1046](https://github.com/uppin/tddy-coder/pull/1046) (`#fs-port` 2/13)

Wiring only: builds `DaemonMetrics`, starts the private listener, hands counters to the services that update them; the public web listener serves no `/metrics`. Covered by the in-process daemon acceptance test. Cross-package entry:
[`docs/dev/changesets/2026-11-26-daemon-metrics-endpoint.md](../../../../docs/dev/changesets/2026-11-26-daemon-metrics-endpoint.md).
