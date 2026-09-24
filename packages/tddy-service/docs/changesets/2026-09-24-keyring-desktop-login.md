# 2026-09-24 — `StartDeviceLogin` / `PollDeviceLogin`, and `auth_flow` in `GetClientConfig`

**Type:** Feature

`#keyring` 2/9, PR [#509](https://github.com/uppin/tddy-coder/pull/509). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-desktop-login.md`](../../../../docs/dev/changesets/2026-09-24-keyring-desktop-login.md)

`auth.proto`: `StartDeviceLogin` (device code, user code, verification URI, expiry, interval) and `PollDeviceLogin` (`DeviceLoginState`: `PENDING`, `SLOW_DOWN` with `interval_seconds`, `DENIED`, `EXPIRED`, `COMPLETE` with the same `session_token` / `user` / `refresh_token` as `ExchangeCodeResponse`). `daemon_config.proto`: `optional string auth_flow = 10` on `GetClientConfigResponse` — `"redirect"`, `"device"`, or absent for no sign-in. The exec-tool tonic supplement stamps `RequestTransport::Grpc`. See [`tddy-github` device-flow.md](../../../tddy-github/docs/device-flow.md).
