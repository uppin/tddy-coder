# Placement, first admission and OS-user resolution (`tddy_daemon_livekit`)

Three small modules that sit with the peer-routing and admission code they read.

| Module | Holds |
|---|---|
| `placement.rs` | `CodebasePlacement` and the `classify_*` functions: where a session's git worktree lives relative to the daemon running its agent (the second placement axis of the remote managed worktree; `daemon_instance_id` decides where the agent runs, this decides whose filesystem holds the worktree). Public, because `tddy-daemon-rpc`'s suites name them |
| `first_admission_token.rs` | `mint_first_admission_token(config, session_admissions, session_id, owning_daemon_instance_id)` |
| `os_user_resolution.rs` | `resolve_os_user(config, user_resolver, session_token)`, re-exported by `tddy-session-lifecycle` as `connection_service::resolve_os_user` |

The two functions take the two or three fields they read rather than a handle; the daemon host passes them
(`handler_state.rs` in `tddy-session-lifecycle`).
