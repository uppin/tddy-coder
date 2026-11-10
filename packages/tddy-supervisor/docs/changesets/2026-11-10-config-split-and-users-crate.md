# 2026-11-10 — config split, spawn_broker extractions, shared users.yaml crate

**Type:** Refactor

`#agent-tools` 33/33 ([#876](https://github.com/uppin/tddy-coder/pull/876)).

- `config.rs` → `config/trust.rs` (the ownership checks) and `config/users.rs` (`UsersConfig`,
  `default_user_sockets_dir`, `refuse_old_user_list_keys`). Code issue `oversized-file-config`
  **closed and deleted**: 507 production lines by its record (502 by the gate) → **378**.
- `spawn_broker.rs` → `spawn_broker/process_syscalls.rs` (parent-death signal, process group,
  privilege drop, chdir, cgroup join) and `spawn_broker/spawn_plan.rs` (`SpawnPlan`, `ChildOutput`,
  `EnvironmentBase`, `PreExecStep`, `pre_exec_plan` and the mount helpers): 984 → 534 production
  lines. The namespace and resolve ranges (plans 09/10) are deferred by the user to a Linux target,
  so record `oversized-file-spawn-broker` stays open, narrowed to 534. Two Linux-only defects the
  macOS-run moves could not see were fixed by hand: the Linux `set_parent_death_signal` widened to
  `pub(crate)` with its `ORPHANED_EXIT_STATUS` import, and the Linux-only test import of
  `EnvironmentBase` / `SpawnPlan` in `supervisor::tests`.
- `users_file.rs`: `UsersFile` wraps `tddy_users_file::UsersMapping`, so the supervisor and the
  daemon apply one validation; the wording is the shared crate's, prefixed with `users.yaml: `.
- Functions brought under the length, depth and complexity budgets (lines/depth, merge-base → head,
  brace matching; none had a record): `run` 111/4 → 48/2, `UsersWatch::spawn` 83/5 → 52/3,
  `UsersFile::from_yaml` 63/6 → 14/3, `open_event_stream` 66 → 44/3, `resolve_spawn` → 30,
  `apply_users_change` → 35/2, `ChildSetup::run` → 16/2, `spawn_now` → 30,
  `ChildEnvironment::build` 78 → 58/2, `Supervisor::new` 80 → 22.
- Still open, deferred by the user (2026-11-10): `for_each_line` (23 lines, depth 5) and
  `forward_lines_until_exit` (20, depth 5) in `streamed.rs`; `build_service_slots` (65) and
  `start_reaper_and_users_watch` (61), the helpers `Supervisor::new` and `run` were split into.
