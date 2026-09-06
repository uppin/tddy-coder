# 2026-08-15 — LiveKit connect/streamer duplication still outstanding

**Category:** Future enhancement
**Source:** remote-git-repo-over-livekit changeset, 2026-08-15

The remote-git changeset deduplicated **one** LiveKit sequence — connect a room, wait for the
serving participant, vend a client through `LiveKitRpcClientFactory` — into
`tddy_livekit::client_connect::connect_client` (`packages/tddy-livekit/src/client_connect.rs`).
Three call sites now share it: `tddy-tools/src/pty_relay.rs:375`,
`tddy-tools/src/session_tool_client.rs:475`, and `tddy-remote-git-repo/src/relay.rs:49`. Two
adjacent duplications were left in place, both out of scope for that changeset:

- **`tddy-screenshare` and `tddy-livekit-screen-capture` are near-duplicate crates.**
  `packages/tddy-screenshare/src/streamer.rs` (183 lines) and
  `packages/tddy-livekit-screen-capture/src/streamer.rs` (176 lines) each implement the same
  shape — `start()` doing `Room::connect` plus a room-event loop, `push_rgba_frame`, `stop` —
  against the same LiveKit API. They cannot use `connect_client` as-is (they publish a track
  rather than address an RPC peer, so they need no target identity), but the connect-and-watch-
  events half is common, and having two crates own it means a fix to one silently misses the
  other. Decide whether one crate supersedes the other, or extract the shared half; do **not**
  add a third.
- **Ten test files under `packages/*/tests` roll their own connect + `ParticipantConnected`
  wait.** These are the copies most likely to carry the silent-degrade bug `connect_client`
  fixed: a closed event channel is indistinguishable from the participant arriving unless the
  wait reports *why* it stopped (`client_connect.rs:63-91`). A test helper wrapping
  `connect_client` would retire them. Note that two of these suites
  (`coder_serves_connection_service_from_participant`, `common_room_set_metadata_handshake_repro`)
  are already the workspace run's timing-flakiest, failing only under parallel load.
