# 2026-11-09 — The runtime serves the per-user daemon sockets; UpdateConfig writes no mapping

**Type:** Architecture

`#agent-tools` 31/32 ([#859](https://github.com/uppin/tddy-coder/pull/859)).

- `runtime/build_context.rs`: a binary host with `TDDY_USERS_FILE` set gets `AgentToolSocketPlacement::Supervised`.
- `runtime/mount_host.rs`: adopts the `user.<os user>` listeners (`adopt_user_sockets_from_environment`) before the local listener clears `LISTEN_*`, and builds the host with `with_host_sessions_merged(host != Embedded)`.
- `daemon_config_service.rs`: `UpdateConfig` never writes `users:` into `daemon.yaml` and writes no `users.yaml` (its message carries no mapping); enrolment is `users.yaml`'s one writer.
- Tests: `tests/daemon_config_service.rs` (`update_config_writes_no_users_key_into_daemon_yaml`), fixtures write `users.yaml`.
- Docs: [daemon-endpoint.md](../daemon-endpoint.md).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-user-mapping-in-users-yaml.md).
