# PRD: Prometheus `/metrics` on a private daemon listener

**Date:** 2026-11-26 · **Type:** Feature · **Streak:** `#fs-port` n2 of 13 (standalone)

## Affected features
[daemon-settings.md](../daemon-settings.md) (new `metrics:` section).

## Summary
Operators cannot observe a running daemon except through logs (the 2026-11-13 LiveKit fd leak was found
by hand). The daemon exposes Prometheus metrics on a **separate, private listener** configured on its own,
never on the public web port.

## Proposed changes
```yaml
metrics:
  listen: "127.0.0.1:9464"   # absent ⇒ no metrics listener at all
```
- `GET /metrics` on that listener only, `text/plain; version=0.0.4` / OpenMetrics; every other path 404.
- The web listener does **not** serve `/metrics`.
- Startup fails with a named error if `metrics.listen` resolves to the same address/port as `listen.web_port`.
- Initial metrics: `tddy_build_info{version}`, `process_open_fds`, `tddy_sessions_active{kind}`,
  `tddy_session_starts_total{result}`, `tddy_rpc_requests_total{service,method,code}`,
  `tddy_rpc_request_duration_seconds` (histogram), `tddy_livekit_rooms_connected`.

## Acceptance criteria
- [ ] With `metrics.listen` set, `GET http://<listen>/metrics` returns all initial metric families.
- [ ] `GET <web>/metrics` is not served.
- [ ] Without `metrics:`, no extra port is bound.
- [ ] Same port as web ⇒ daemon refuses to start, naming both settings.
- [ ] An RPC call increments `tddy_rpc_requests_total` for its service/method/code.
