# 2026-11-09 — users.yaml is the daemon's mapping source; one daemon socket per OS user

**Type:** Architecture

`#agent-tools` 31/32 ([#859](https://github.com/uppin/tddy-coder/pull/859)).

- `users_file.rs`: `UsersFileSource::{HandedBySupervisor, BesideConfig}` (`resolve`, `from_environment`), `load_users_file` (supervised: root-owned and closed to group/others; no `root`; no login mapped twice), `USERS_FILE_ENV` = `TDDY_USERS_FILE`, `USERS_FILE_NAME`. A missing file fails the load naming it.
- `DaemonConfig::load` refuses a `users:` key (`refuse_users_key`) and fills `config.users` from `users.yaml`; the field is `skip_serializing`.
- `LiveUsers::owned_by_supervisor`, `is_owned_by_supervisor`; `EnrolmentRefusal::OwnedBySupervisor`. `enrol_first_login` writes the `users.yaml` beside the config; `first_login_enrolment::enrol_first_login(users_path, …)` refuses a missing file `ConfigNotWritable` and leaves `daemon.yaml` untouched.
- `agent_tool_sockets.rs`: `user_socket_path_for_user` (`<data>/run/<u>/daemon.sock`), `USER_SOCKET_FD_NAME_PREFIX` = `user.`, `user_socket_fds`, `adopt_user_sockets_from_environment`, `adopt_user_descriptors`, `AgentToolSocketPlacement::Supervised`, `UserScopedService` + `install_user_scoped_service`. Removed `agent_tool_socket_path_for_user`, `AGENT_TOOLS_FD_NAME_PREFIX`, `agent_tool_fds`, `adopt_from_environment`, `adopt_descriptors`.
- `owner_socket.rs`: `OwnerSocketKind::USER_DAEMON`; `AGENT_TOOL` removed.
- Tests: `users_file.rs`, `config.rs`, `live_users.rs` (incl. a missing `users.yaml` refused with nothing written), `agent_tool_sockets_tests.rs`, `tests/first_login_enrolment_acceptance.rs`.
- Docs: [daemon-kernel.md](../daemon-kernel.md).

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-user-mapping-in-users-yaml.md).
