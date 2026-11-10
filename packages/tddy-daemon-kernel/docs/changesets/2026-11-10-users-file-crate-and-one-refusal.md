# 2026-11-10 — users.yaml through the shared tddy-users-file crate; one owner-socket refusal

**Type:** Refactor

`#agent-tools` 33/33 ([#876](https://github.com/uppin/tddy-coder/pull/876)).

- `users_file::load_users_file` parses with `tddy_users_file::UsersMapping::from_yaml` and maps
  `distinct_rows()` into `UserMapping` (61/5 → 21/1). The daemon refuses an OS user that cannot name a
  socket, a row with no login and more than 15 distinct OS users, as the supervisor does. The
  root-ownership trust check (`require_root_owned_users_file`) stays with the daemon.
- `owner_socket::not_privileged_to_hand_over(kind, euid, os_user, target_uid)` is the one refusal for
  both socket kinds, naming `users.yaml` and no `supervisor.yaml` key; `OwnerSocketKind::setting` is
  deleted. `bind_owner_only` 74/5 → 46/2 (`socket_parent_dirs` extracted).
- `agent_tool_sockets::ensure_bound_blocking` 77 → 50.
