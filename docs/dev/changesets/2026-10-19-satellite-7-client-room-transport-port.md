# 2026-10-19 — Client room transport through a port

**Type:** Architecture

`#satellite` 7/13 ([#629](https://github.com/uppin/tddy-coder/pull/629)). Client tools join a room through a
port; only their binaries name LiveKit. `tddy-session-sync`, `tddy-remote-git-repo`, `tddy-terminal-rpc` and
`tddy-session-tool-client` leave the ratchet's `KNOWN_OFFENDERS`.

## What changed

- **Port crate.** `tddy-room-client`: `RoomJoin`, `RoomAuth` (`Credentials` or `Token`), `RoomConnector`,
  `RoomSession` (`rpc`, `open_duplex`, `subscribe`, `peer_present`), `RoomDuplex`, `RoomConnectError`
  (with `Display`) and the `NoRoomTransport` null object, plus an `in_memory` test connector behind the
  `test-support` feature. [Room client](../../packages/tddy-room-client/docs/room-client.md).
- **Plugin.** `tddy_livekit::room_connector::LiveKitRoomConnector` mints the token, joins through
  `connect_client`, and wraps the room as a session.
- **Libraries.** `tddy_session_sync::attach(credentials, connector)` and `join_session_room` (`AttachError::Token`
  is gone: a mint failure is a `Room` error); `tddy_remote_git_repo::relay::run(request, credentials, connector)`
  with `relay_over_duplex` as the stdio seam; `tddy_terminal_rpc::pty_relay::run_pty_relay(config, connector)`
  with `join_terminal_room`; the tool client's `install_room_connector` / `room_session_via`.
- **Binaries.** `tddy-session-sync`, `tddy-remote-git-repo` and `tddy-tools` (behind its `livekit` feature,
  otherwise `NoRoomTransport`) wire `LiveKitRoomConnector`.
- **Features.** The `livekit` features on `tddy-terminal-rpc`, `tddy-session-tool-client` and `tddy-discovery`
  are removed; their optional `tddy-livekit`/`livekit` dependencies with them.
- **Ratchet.** Four crates struck from `KNOWN_OFFENDERS`; `WIRING_WITH_A_LIBRARY` (`tddy-remote-git-repo`,
  `tddy-session-sync`, `tddy-tools`) added, with a shape test that holds those libraries' sources free of
  LiveKit paths. [Extension ports](../../packages/tddy-daemon-extensions/docs/extension-ports.md#the-satellite-rule-and-its-ratchet).

## Not changed

Identities, room names, token lifetimes (six hours for a mirror, one hour for a relay), the participant
waits, and the error wording for an unreachable room and an absent daemon. The tool client's three transport
selections are still repeated (backlog entry *session-tool-client repeats its transport selection three
times*). The streamer and test-helper duplication in *LiveKit connect/streamer duplication still outstanding*
is untouched; that entry stays. `tddy-coder` and the daemon side are not touched.

## Verification

Scoped, not whole-workspace. `cargo clippy --all-targets -D warnings` is clean on the nine touched packages plus
`tddy-session-lifecycle` and `tddy-daemon-rpc`; `cargo test` passes on `tddy-room-client`, `tddy-session-sync`,
`tddy-remote-git-repo`, `tddy-terminal-rpc`, `tddy-session-tool-client`, `tddy-discovery`, `tddy-tools` and
`tddy-daemon-extensions`; `tddy-livekit`'s `room_connector` suite (six tests) and `tddy-session-lifecycle`'s
`session_sync_livekit_acceptance` (six tests) pass against a real LiveKit server.

## Code issues

Three over-budget files grew and their splits are deferred with the developer's consent:
`oversized-file-attach` 536 → 555, `oversized-file-lib` (session-tool-client) 1,111 → 1,114,
`oversized-file-pty-relay` 776 → 784. `sync.rs` (788, unchanged) has a new record, `oversized-file-sync`.
All four stay open. Closed backlog entries: none.
