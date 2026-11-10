# 2026-11-10 — the daemon reads users.yaml through the shared tddy-users-file crate

**Type:** Refactor

`#agent-tools` 33/33 ([#876](https://github.com/uppin/tddy-coder/pull/876)).

- `tddy-daemon-kernel::users_file::load_users_file` parses with
  `tddy_users_file::UsersMapping::from_yaml` and maps `distinct_rows()` into `UserMapping`; the
  daemon now refuses an unsafe OS user, an empty login and more than 15 distinct OS users itself
  (previously only root-mapped and ambiguous rows). The root-ownership trust check is extracted
  (`require_root_owned_users_file`) and stays with the daemon.
- `owner_socket`: one `not_privileged_to_hand_over` wording for both socket kinds, naming
  `users.yaml`; `OwnerSocketKind::setting` deleted.
- Tier C: extracts in `runtime/build_context.rs` and `runtime/mount_host.rs`.
