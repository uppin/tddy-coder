# 2026-10-20 — Split placement through the split-rooms port

`#satellite` 12/13 (#634). The crate names no LiveKit crate. `SplitAgentRoom::issued_by` asks
`SplitRoomHosting::agent_join` for the agent's join, and `host_split_room` opens the room through
`host_remote_worktree`. `SplitSessions` holds `split_rooms` and `reachable_sessions`.
`RoomPollTokenMinter` and `HostWorktreeSnapshots` implement the extension crate's
`SessionTokenMinter` and `RemoteSnapshotSource`. See [module layout](../module-layout.md).

Code issues re-measured, all unchanged: `spawn_split_agent` 118 lines,
`start_split_claude_cli_session` 149, `delete_paired_codebase_session` 96.
