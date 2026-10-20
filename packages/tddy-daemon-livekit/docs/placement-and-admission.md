# Placement and OS-user resolution (`tddy_daemon_livekit`)

Two small modules that sit with the peer-routing and admission code they read. The first admission
token is minted in `tddy-session-agents` (`room_access::mint_first_admission`), through the
`RoomTokenMinter` and `RoomAdmission` ports ([extension ports](../../tddy-daemon-extensions/docs/extension-ports.md)).

| Module | Holds |
|---|---|
| `placement.rs` | `CodebasePlacement` and the `classify_*` functions: where a session's git worktree lives relative to the daemon running its agent (the second placement axis of the remote managed worktree; `daemon_instance_id` decides where the agent runs, this decides whose filesystem holds the worktree). Public, because `tddy-daemon-rpc`'s suites name them |
| `os_user_resolution.rs` | `resolve_os_user(config, user_resolver, session_token)`, re-exported by `tddy-session-lifecycle` as `connection_service::resolve_os_user` |

`resolve_os_user` takes the fields it reads rather than a handle; the daemon host passes them
(`handler_state.rs` in `tddy-session-lifecycle`).
