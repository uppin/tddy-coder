# 2026-11-10 — agent-tools sweep: file splits, the shared users.yaml crate, dead-code deletion

**Type:** Refactor

`#agent-tools` 33/33 ([#876](https://github.com/uppin/tddy-coder/pull/876)), the stack's sweep node,
cut from `master` after #835 → #865 merged. Every code move went through `tddy-tools restructure`
(one plan file per seam group), and each applied plan was checked with `restructure verify` so every
statement is accounted for. Hand edits are limited to the approved dead-module deletion, the comment
rewording, the new crate's code, and post-move build fixes.

## What changed

- **Spec split (B1):** `tddy-coding-agent-specs` `spec.rs` → `spec/{headless_section, acp,
  slot_shapes, route_slot, pass_env, argv}.rs`.
- **Supervisor config split (B3):** `config.rs` → `config/trust.rs`, `config/users.rs`.
- **spawn_broker (B2, partial):** `spawn_broker/process_syscalls.rs` and `spawn_broker/spawn_plan.rs`
  (plans 11/12). Plans 09 (namespaces) and 10 (resolve) are deferred by the user, 2026-11-10: they
  are Linux-only ranges the engine cannot move on macOS. Two Linux-only defects the macOS-run moves
  left were fixed by hand (`set_parent_death_signal`'s Linux arm visibility and import; a Linux-only
  test import in `supervisor::tests`).
- **Shared users crate (B8):** the new workspace crate `tddy-users-file` (serde/serde_yaml only)
  holds `UsersMapping::from_yaml`, `distinct_rows` and `is_one_safe_segment`. Both readers parse
  through it: `tddy-supervisor`'s `UsersFile` and `tddy-daemon-kernel`'s `load_users_file`. The
  daemon therefore refuses an unsafe OS user, an empty login and more than 15 distinct OS users. Each
  reader keeps its own trust check.
- **Dead adoption path (B5):** `tddy-agent-launch`'s `inherited_host_sockets.rs`,
  `HostSessionSockets::adopt_inherited` and their adoption tests are deleted (user-approved). The
  three merged user-socket tests remain in `connection_service/user_socket_tests.rs`. This resolves
  the backlog entry `2026-11-09-dead-inherited-host-session-socket-adoption`, which is deleted.
- **Wording (B6/B7):** the stale `allowed_env_keys` / `allowed_relay_tool_paths` comments now read
  `exec_roots` and whole-env. `codex_acp_route_args` is renamed `toml_route_overrides` (#857's
  `TODO(sweep)`). `OwnerSocketKind::setting` is deleted, leaving one `users.yaml` refusal for both
  socket kinds.
- **Function extracts (B4 / Tier C):** `extract_method` plans across supervisor, specs, daemon-kernel,
  agent-launch, daemon, pty, terminal-rpc, session-split, cli-sessions, coder, coding-subagent and
  session-lifecycle. The per-package entries carry the before → after numbers.

## Code issues closed (final measurement)

| Record | Final |
|---|---|
| `tddy-coding-agent-specs` `oversized-file-spec` | 1,111 (record) / 1,152 (gate) → **378** production lines, deleted |
| `tddy-supervisor` `oversized-file-config` | 507 (record) / 502 (gate) → **378** production lines, deleted |

## Still open, deferred by the user (2026-11-10)

- **`oversized-file-spawn-broker`:** narrowed from 984 to 534. Plans 09/10 are to run on a Linux
  target; see the todo entry `2026-11-10-spawn-broker-linux-only-moves`.
- **Six functions the engine refused during green:** `build_session_host` (74), `DaemonSessionHost::new`
  (97), `spawn_tool_on` (75), `for_each_line` (depth 5), `forward_lines_until_exit` (depth 5) and
  `UsersMapping::from_yaml` (depth about 6). See the todo entry
  `2026-11-10-sweep-deferred-function-extractions`.
- **Files and helpers measured over budget at the wrap:** the extracts pushed three files past 500 —
  `tddy-pty` `runtime.rs` (451 → 523), `svc_host_builders.rs` (493 → 531) and
  `svc_spawn_split_agent.rs` (497 → 575). Five extracted helpers are over a function threshold:
  `spawn_child_and_pump` (67), `spawn_pty_writer` (depth 5), `build_service_slots` (65),
  `spawn_stdin_pump` (65) and `start_reaper_and_users_watch` (61). See the todo entry
  `2026-11-10-sweep-wrap-deferred-splits-and-extracted-helpers`.
- **The `install` split** (1,811 lines) is deferred again.

Each open item has a `code-issues/` record carrying the consent line.

See the per-package entries of the same date in `tddy-coding-agent-specs`, `tddy-supervisor`,
`tddy-daemon-kernel`, `tddy-daemon`, `tddy-agent-launch`, `tddy-agent-backend` and `tddy-users-file`.
