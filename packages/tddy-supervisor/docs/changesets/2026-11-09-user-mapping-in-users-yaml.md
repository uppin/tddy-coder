# 2026-11-09 — users.yaml: the user mapping, one daemon socket per mapped user, watch and restart

**Type:** Architecture

`#agent-tools` 31/32 ([#859](https://github.com/uppin/tddy-coder/pull/859)).

- `users_file.rs`: `UsersFile::load` / `from_yaml`, `MappedUser`, `USERS_FILE_NAME`, `MAX_MAPPED_OS_USERS` = 15. Trust check shared with `SupervisorConfig::load` (`require_trustworthy_config_*`, now `pub(crate)`); no `root`, no login mapped to two OS users, plain-segment OS users.
- `config.rs`: top-level `users: {file, sockets_dir}` (`UsersConfig`, default `/run/tddy-users`), `ManagedService::serves_users`; `refuse_old_user_list_keys` refuses `allowed_session_users`, `host_sockets`, `agent_tool_sockets` naming `users.yaml`. `host_socket_config::validate` checks only that a `serves_users` service has `users:` and its own `socket:`. `SpawnPolicy.allowed_session_users` removed.
- `user_sockets.rs`: `UserListeners::bind` / `rebuild` (unchanged users keep the same `Arc`), `user_socket_path` = `<sockets_dir>/<u>/daemon.sock`.
- `handover.rs` / `socket.rs`: `SocketHandover::with_user_socket` and `users_file`; `USER_SOCKET_FD_NAME_PREFIX` = `user.`. Removed `with_host_session_socket`, `with_agent_tool_socket`, `HOST_SESSION_FD_NAME_PREFIX`, `AGENT_TOOLS_FD_NAME_PREFIX`, `host_session_fd_name`, `agent_tools_fd_name`, `resolve_host_session_fds`.
- `spawn_broker.rs`: `USERS_FILE_ENV` = `TDDY_USERS_FILE`, set by `ChildEnvironment` after the `env:` overlay.
- `policy::resolve_mapped_session_user` replaces `resolve_session_user`; `server.rs` reads `Supervisor::current_users()`.
- `users_watch.rs` (`notify`, new dependency): watch on the file's directory, refuses to start if the path cannot be canonicalized; valid edit → `UsersWatchSink::changed`, invalid → `refused`, mapping kept. `lib.rs` applies changes one at a time. `supervisor/users_change.rs`: `apply_users_change` rebuilds, then stops each `serves_users` service (SIGTERM, then SIGKILL, 10 s each; a survivor is an error, not restarted) and starts it.
- Tests: `users_file.rs`, `user_sockets.rs`, `handover.rs`, `policy.rs`, `config.rs`, `users_watch.rs`, `supervisor.rs`; Linux-only `tests/supervisor_user_sockets.rs`; `tests/support/mod.rs` writes `users.yaml`.
- Code issues: `oversized-file-spawn-broker` 977 → 984 (not split); `oversized-file-config` added, `config.rs` 463 → 507; `supervisor.rs` 388 → 422, still under budget.
- Docs: [host-sockets.md](../host-sockets.md), [architecture.md](../architecture.md).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-user-mapping-in-users-yaml.md).
