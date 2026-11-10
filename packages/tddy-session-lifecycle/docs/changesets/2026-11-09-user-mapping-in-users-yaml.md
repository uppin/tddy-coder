# 2026-11-09 — Host sessions merged onto the per-user daemon socket at build

**Type:** Architecture

`#agent-tools` 31/32 ([#859](https://github.com/uppin/tddy-coder/pull/859)).

- `svc_host_builders.rs`: `with_host_sessions_merged(merged)` serves the host-session service on the per-user sockets for a binary host; an embedded host binds its own host sockets. `with_inherited_host_session_sockets` and `with_host_sessions_on_user_sockets` removed.
- `inherited_host_sockets_tests.rs`: `a_supervised_daemon_serves_host_session_and_agent_tool_calls_on_one_user_socket`, `a_supervised_host_session_refusal_names_users_yaml`, and `the_supervisor_and_the_daemon_share_one_user_socket_prefix` (pins `user.` and `TDDY_USERS_FILE` across crates, replacing the two-prefix pin).
- Test fixtures across ~60 suites write `users.yaml` beside their config.
- Docs: [session-identity.md](../session-identity.md), [coding-agent-launcher.md](../coding-agent-launcher.md), [module-layout.md](../module-layout.md).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-user-mapping-in-users-yaml.md).
