# 2026-11-26 — Prometheus `/metrics` on a private listener

Operators can observe a running daemon: with `metrics.listen` set, `GET http://<listen>/metrics` serves the OpenMetrics exposition; every other path 404s, and the web listener never serves `/metrics`. Absent `metrics:` binds nothing; a port equal to the web port refuses startup naming both settings. See the [daemon settings](../daemon-settings.md) and the cross-package entry [`docs/dev/changesets/2026-11-26-daemon-metrics-endpoint.md`](../../../docs/dev/changesets/2026-11-26-daemon-metrics-endpoint.md) (PR #1046).
