# 2026-08-15 — remote-git-repo-over-livekit

**Type:** Refactor

`pty_relay.rs` and `session_tool_client.rs` drop their private copies of the LiveKit join + participant-wait sequence for `tddy_livekit::client_connect`, inheriting its fix for a closed event channel reading as "participant appeared". `pty_relay.rs` also moves from `RpcClient::new_shared` + its own `room.subscribe()` to the shared factory: it creates one room and one client per invocation, which is the case `session_tool_client.rs` already documented as equivalent. (tddy-tools)
