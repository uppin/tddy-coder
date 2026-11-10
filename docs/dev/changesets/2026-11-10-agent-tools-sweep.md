# 2026-11-10 — agent-tools sweep: file splits, the shared users.yaml crate, dead-code deletion

**Type:** Refactor

`#agent-tools` 33/33 ([#876](https://github.com/uppin/tddy-coder/pull/876)), branch
`feature/agent-tools/sweep`. Every code move went through `tddy-tools restructure` (plans in
`docs/dev/1-WIP/2026-11-10-agent-tools-sweep-plans/`), each applied plan verified with
`restructure verify` so every statement is accounted for; hand edits are limited to the approved
dead-module deletion, the comment rewording, the new crate's code, and noted post-move build fixes.

- **Spec split (B1):** `spec.rs` 1111 → 378 production lines into `spec/headless_section.rs`,
  `spec/acp.rs`, `spec/slot_shapes.rs`, `spec/route_slot.rs`, `spec/pass_env.rs`, `spec/argv.rs`
  (plans 01–06 with `-b`/`-c`). Oversized record `oversized-file-spec` closed at 378 and deleted.
- **Supervisor config split (B3):** `config.rs` 728 → 378 into `config/trust.rs` and
  `config/users.rs` (`UsersConfig`, `refuse_old_user_list_keys`). Record `oversized-file-config`
  closed at 378 and deleted.
- **spawn_broker (B2, partial):** plans 11 (`spawn_broker/process_syscalls.rs`) and 12
  (`spawn_broker/spawn_plan.rs`) applied; `spawn_broker.rs` 984 → 534. Plans 09 (namespaces) and
  10 (resolve) deferred by the user, 2026-11-10 (#876 6e): Linux-only ranges the engine cannot
  move on macOS; follow-up on a Linux target (`docs/dev/todo/2026-11-10-spawn-broker-linux-only-moves.md`).
  Record `oversized-file-spawn-broker` narrowed to 534, not closed.
- **Shared users crate (B8):** new workspace crate `tddy-users-file` (serde/serde_yaml only):
  `UsersMapping::from_yaml` with the supervisor's checks and wording, `distinct_rows`,
  `is_one_safe_segment`. `tddy-supervisor`'s `UsersFile` wraps it; `tddy-daemon-kernel`'s
  `load_users_file` maps `distinct_rows()` into `UserMapping`, so the daemon now also refuses an
  unsafe OS user, an empty login and more than 15 distinct OS users.
- **Dead adoption path (B5):** `packages/tddy-agent-launch/src/inherited_host_sockets.rs`,
  `HostSessionSockets::adopt_inherited` and its adoption tests deleted (user-approved); the three
  merged user-socket tests remain in the renamed `user_socket_tests.rs` module.
- **Wording (B6/B7):** stale `allowed_env_keys` / `allowed_relay_tool_paths` comments now read
  `exec_roots` / whole-env; `codex_acp_route_args` renamed to `toml_route_overrides` (plan 13);
  `OwnerSocketKind::setting` deleted, one `users.yaml` refusal wording for both socket kinds.
- **Tier C / B4 extracts:** 30+ `extract_method` plans across supervisor, specs, daemon-kernel,
  agent-launch, daemon, pty, terminal-rpc, session-split, cli-sessions, coder, coding-subagent
  and session-lifecycle brought their functions to ≤60 lines (depth ≤4, complexity ≤5) except the
  engine-blocked seams listed in the WIP changeset (composition roots whose field types the file
  does not import, and the streamed/pty seams whose ranges hold early returns or unimported
  portable-pty type names).

See the per-package entries in `tddy-coding-agent-specs`, `tddy-supervisor`, `tddy-daemon` and
`tddy-users-file`.
