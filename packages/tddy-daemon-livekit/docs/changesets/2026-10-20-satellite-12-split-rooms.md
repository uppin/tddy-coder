# 2026-10-20 — `LiveKitSplitRooms`

`#satellite` 12/13 (#634). `split_rooms::LiveKitSplitRooms` implements `SplitRoomHosting` over the
registry, `RemoteCheckout` and the `RoomTokenMinter`. `session_room` re-exports `WorktreeSnapshot`,
`SessionTokenMinter` and `RemoteSnapshotSource` from `tddy_daemon_extensions::split_rooms`.
`tests/split_rooms.rs` verifies the agent's join with LiveKit's verifier. `session_room.rs`:
2,885 → 2,857 production lines. See [session room](../session-room.md).
