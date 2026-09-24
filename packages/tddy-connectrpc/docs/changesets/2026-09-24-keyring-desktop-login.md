# 2026-09-24 — The `/rpc` router stamps `RequestTransport::Http`

**Type:** Feature

`#keyring` 2/9, PR [#509](https://github.com/uppin/tddy-coder/pull/509). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-desktop-login.md`](../../../../docs/dev/changesets/2026-09-24-keyring-desktop-login.md)

Every request the Connect router hands the bridge carries `RequestMetadata::over(RequestTransport::Http)` (`over_http`), stamped in the router rather than read from any header a client could set. See [`tddy-rpc` request-transport.md](../../../tddy-rpc/docs/request-transport.md).
