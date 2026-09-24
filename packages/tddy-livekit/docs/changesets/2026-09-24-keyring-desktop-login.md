# 2026-09-24 — `LiveKitParticipant` stamps `RequestTransport::LiveKit`

**Type:** Feature

`#keyring` 2/9, PR [#509](https://github.com/uppin/tddy-coder/pull/509). Cross-package entry: [`docs/dev/changesets/2026-09-24-keyring-desktop-login.md`](../../../../docs/dev/changesets/2026-09-24-keyring-desktop-login.md)

`LiveKitParticipant::connect` and `::join` build their `ServerEngine` with `RequestTransport::LiveKit`, so every request from a room — the common room, a session room, a peer daemon's forward — carries that stamp whatever its envelope claims. See [`tddy-rpc` request-transport.md](../../../tddy-rpc/docs/request-transport.md).
