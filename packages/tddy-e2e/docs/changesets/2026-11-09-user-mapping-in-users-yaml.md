# 2026-11-09 — Install tests: the one-time users.yaml migration and a fresh users: block

**Type:** Architecture

`#agent-tools` 31/32 ([#859](https://github.com/uppin/tddy-coder/pull/859)).

- `tests/install_supervisor.rs`: `install_moves_daemon_yaml_users_into_users_yaml_once_and_drops_the_old_keys` (idempotent) and `install_declares_no_per_user_sockets_in_supervisor_yaml` (`users:` block, `serves_users`, empty `users.yaml` at `0644`). The four agent-tool-socket install tests were deleted with approval. `tests/install_script.rs` follows the template change.

See the [cross-package entry](../../../../docs/dev/changesets/2026-11-09-user-mapping-in-users-yaml.md).
