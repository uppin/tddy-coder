# 2026-11-10 — config split, spawn_broker extractions, shared users.yaml crate

**Type:** Refactor

`#agent-tools` 33/33 ([#876](https://github.com/uppin/tddy-coder/pull/876)).

- `config.rs` 728 → 378 production lines: `config/trust.rs` (ownership checks) and `config/users.rs`
  (`UsersConfig`, `refuse_old_user_list_keys`); record `oversized-file-config` closed and deleted.
- `spawn_broker.rs` 984 → 534: `spawn_broker/process_syscalls.rs` and `spawn_broker/spawn_plan.rs`
  extracted (plans 11/12). The namespaces and resolve ranges (plans 09/10) are deferred by the user
  to a Linux target; record `oversized-file-spawn-broker` narrowed to 534.
- `users_file.rs`: `UsersFile` wraps `tddy_users_file::UsersMapping`; validation wording is the
  shared crate's, prefixed with `users.yaml: ` where it does not already name the file.
- Tier C/B4: extracts in `lib.rs` (run 111 → 48), `users_watch.rs`, `client.rs`, `streamed.rs`
  (forwarding), `server.rs`, `supervisor.rs`, `supervisor/users_change.rs`,
  `spawn_broker/child_environment.rs` and `spawn_broker/fork_broker.rs`.
