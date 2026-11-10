# Initial discovery — `#agent-tools` sweep (n33)

**Date:** 2026-11-10 · **Measured on:** the top node's head `b8d9b769c` against the stack's merge-base
`mb = 153b3c05d`. **Branch:** `feature/agent-tools/sweep`, re-cut from `origin/master` `20f475576` after
the stack squash-merged (04:24Z). `packages/` is byte-identical between the two.

This file holds the traces behind the changeset
[`2026-11-10-agent-tools-sweep.md`](./2026-11-10-agent-tools-sweep.md). Every number here was measured on
this branch's tree, base `153b3c05d` vs head `b8d9b769c`.

## 1. What the stack added to the records

```bash
mb=153b3c05d
git diff --name-status $mb..HEAD -- docs/dev/todo/ ':(glob)packages/*/docs/code-issues/**'
```

| Change | Record |
|---|---|
| M (regressed 981 → 1152) | `packages/tddy-coding-agent-specs/docs/code-issues/oversized-file-spec.md` |
| R, body unchanged (78 lines, depth 5) | `packages/tddy-coding-subagent/docs/code-issues/complexity-subagent-session-take-turn.md` |
| D (closed by #835) | `packages/tddy-coding-subagent/docs/code-issues/oversized-file-session.md` |
| M, row only (579 → 579) | `packages/tddy-daemon-auth/docs/code-issues/oversized-file-auth.md` |
| A (463 → 507 by the record; 502 by the gate's awk) | `packages/tddy-supervisor/docs/code-issues/oversized-file-config.md` |
| M (regressed 969 → 984) | `packages/tddy-supervisor/docs/code-issues/oversized-file-spawn-broker.md` |
| M (narrowed, then grew 1,600 → 1,811 `wc -l`) | `docs/dev/todo/2026-11-01-install-script-over-length-budget.md` |
| A | `docs/dev/todo/2026-11-07-setpriv-acp-agent-keeps-daemon-user-logname.md` |
| A | `docs/dev/todo/2026-11-08-install-renders-configured-harness-exec-roots.md` |
| A | `docs/dev/todo/2026-11-09-daemon-codex-acp-login-runs-as-the-daemon-user-in-the-worktree.md` |
| A (deferred by the user 2026-11-09) | `docs/dev/todo/2026-11-09-daemon-creates-no-cgroup-scope-for-supervised-spawns.md` |
| A (deletion approved 2026-11-09) | `docs/dev/todo/2026-11-09-dead-inherited-host-session-socket-adoption.md` |
| A | `docs/dev/todo/2026-11-09-restructure-move-item-defects-found-splitting-coding-subagent-session.md` |
| D ×3 | three TODOs the stack closed (acp hand-built launch path, opencode route-file owner, operator-start undo) |

## 2. Re-measurement — what was run

Packages the stack touched (30): `tddy-agent-backend tddy-agent-launch tddy-cli-sessions tddy-coder
tddy-coding-agent-specs tddy-coding-subagent tddy-daemon tddy-daemon-auth tddy-daemon-kernel tddy-daemon-livekit
tddy-daemon-rpc tddy-daemon-sandbox tddy-desktop tddy-e2e tddy-git-clone tddy-github tddy-host-service
tddy-integration-tests tddy-model-registry tddy-pty tddy-session-agents tddy-session-lifecycle tddy-session-split
tddy-spawn tddy-supervisor tddy-telegram tddy-telegram-control tddy-terminal-rpc tddy-vm-testkit
tddy-worktree-service`.

1. **File length, every touched package**: `/pr-wrap` step 3.5's own loop (`count_prod` awk to the first
   `#[cfg(...test...)]`, `git diff --name-status -M $mb..HEAD`, test-path exclusions), base → head.
2. **Function length and nesting, every touched package**: a brace-matching scan, the same method as the
   existing `complexity-*` records ("hand-measured by brace matching"). It covers all 136 touched production
   `.rs` files under `src/`, stops at an inline `#[cfg(test)] mod … {`, and skips `*_tests.rs`.
   Thresholds: length > 60, brace depth > 4. A function is reported when it is new over a threshold, or
   when it was already over one and got worse. ⚠ Brace depth counts closures and struct literals as
   nesting, so every depth figure below was read by hand before it was listed.
3. **Coverage + CRAP** (`./dev tddy-tools analyze coverage` then `analyze report`), on the packages whose
   records the stack touched:
   | Package | Join | Tests captured | CRAP > 30 |
   |---|---|---|---|
   | `tddy-coding-agent-specs` | 73.5 % (415/565) | — | none (top: `slots::render` 20) |
   | `tddy-coding-subagent` | 68.6 % (279/407) | 81 | `CodingAgentSession::opened` **42** (c 6) |
   | `tddy-daemon-auth` | 79.0 % (407/515) | — | none (top 12) |
   | `tddy-supervisor` | 77.9 % (791/1015) | 277 | 10 functions ≥ 42 (below) |

   ⚠ **Blind spot.** On this host every supervisor CRAP over 30 reads 0 % coverage. The code either runs in
   a forked pre-exec child, which never flushes its counters before `exec`, is Linux-only (`cgroup_broker`),
   or is reached only through the spawned supervisor binary. So the supervisor CRAP numbers measure
   complexity at zero coverage, not missing tests. A complexity of 5 or less is what closes one
   measurably: CRAP is then 30 or less even at 0 %.

### 2a. Files ≥ 500 production lines (the gate's output)

| base → head | File | Record |
|---|---|---|
| 981 → **1152** | `packages/tddy-coding-agent-specs/src/spec.rs` | `oversized-file-spec.md` |
| 969 → **984** | `packages/tddy-supervisor/src/spawn_broker.rs` | `oversized-file-spawn-broker.md` |
| 463 → **502** | `packages/tddy-supervisor/src/config.rs` | `oversized-file-config.md` |
| 579 → 579 | `packages/tddy-daemon-auth/src/auth.rs` | `oversized-file-auth.md` — unchanged, **out of scope** |
| 545 → 540 | `packages/tddy-session-lifecycle/src/test_util.rs` | `oversized-file-test-util.md` — improved, out of scope |
| 1293 → 1301 · 696 → 700 · 823 → 824 · 630 → 630 · 1262 → 1259 | `*_tests.rs` in session-lifecycle, agent-launch, host-service | no record; these are test files the gate's path filter misses (see 6e Q7) |

### 2b. Functions — new findings (no record, crossed a threshold in this stack)

| Function | base → head (lines, depth) | Read by hand |
|---|---|---|
| `tddy-supervisor/src/lib.rs:84` `run` | 66/3 → **111/4** | real: #859/#865 added the users watch, the user-socket survey, and the scope wiring |
| `tddy-supervisor/src/users_watch.rs:37` `UsersWatch::spawn` | new, **83/5** | real: the notify callback is a 50-line closure (reload with bounded retry, compare, publish or refuse) |
| `tddy-supervisor/src/users_file.rs:93` `UsersFile::from_yaml` | new, **63/6** | real: per-row checks plus a `btree_map::Entry` match inside the loop |
| `tddy-supervisor/src/client.rs:244` `open_event_stream` | new, **66**/4 | real: the first-event read and the forwarding task in one body |
| `tddy-supervisor/src/streamed.rs:182` `for_each_line` | new, 32/**6** | real: loop › match › arm › for › if › if |
| `tddy-supervisor/src/streamed.rs:112` `forward_lines_until_exit` | new, 20/**5** | borderline: `select!` › match › arm › if. 4 levels of control flow plus the macro |
| `tddy-daemon-kernel/src/users_file.rs:70` `load_users_file` | new, **61/5** | real. It also duplicates the root/ambiguous-mapping checks of `UsersFile::from_yaml` in the other crate (see 6e Q6) |
| `tddy-daemon-kernel/src/owner_socket.rs:91` `bind_owner_only` | 65/4 → **74/5** | real: crossed the depth threshold |
| `tddy-cli-sessions/src/cli_session_manager/pty_spawn.rs:75` `spawn_tool_on` | new, **104**/3 | real (the file is new in #836) |
| `tddy-coding-subagent/src/session.rs:153` `CodingAgentSession::opened` | CRAP **42** (c 6, 0 %) | real growth (#835 hooks `begin_open`/`end_open`). The 0 % is real for this crate's tests: every region count is 0 |
| `tddy-supervisor/src/supervisor/users_change.rs:26` `apply_users_change` | new, CRAP **56** (c 7) | blind spot (supervisor binary). c 7 is real |
| `tddy-supervisor/src/server.rs:171` `resolve_spawn` | +13 lines, CRAP **42** (c 6) | #865's one gate. c 6 is real |
| `tddy-supervisor/src/spawn_broker/fork_broker.rs:239` `ChildSetup::run` | +7 lines, CRAP **72** (c 8) | b8d9b769c's `FD_CLOEXEC` clear. Runs in the forked child, so the coverage is a blind spot |
| `tddy-supervisor/src/spawn_broker/fork_broker.rs:122` `spawn_now` | +4 lines, CRAP **42** (c 6) | blind spot. c 6 is real |
| `tddy-supervisor/src/lib.rs:84` `run` (CRAP) | CRAP **156** (c 12) | the same function as row 1 |

### 2c. Functions — pre-existing over-budget functions the stack made worse

| Function | base → head (lines, depth) | Existing record |
|---|---|---|
| `tddy-agent-launch/src/host_session_socket.rs:191` `ensure_bound` | 65 → 75 | none |
| `tddy-agent-launch/src/opencode_cli_spawn.rs:113` `start_cli_harness_session` | 302 → 303 | `complexity-opencode-cli-spawn-start-cli-harness-session.md` |
| `tddy-agent-launch/src/opencode_cli_spawn/harness_launch.rs:89` `launch` | 69 → 74 | none |
| `tddy-agent-launch/src/opencode_cli_spawn/resume.rs:160` `resume_cli_harness_session` | 117 → 118 | `complexity-opencode-cli-spawn-resume-resume-cli-harness-session.md` |
| `tddy-agent-launch/src/svc_resume_sandboxed_cli_harness_session.rs:145` `relaunch_sandboxed_cli_harness` | 139 → 140 | `complexity-svc-resume-sandboxed-cli-harness-session-relaunch-sandboxed-cli-harness.md` |
| `tddy-agent-launch/src/svc_signal_delete_session.rs:90` `delete_session_as` | 90 → 94 | none |
| `tddy-agent-launch/src/svc_start_sandboxed_cli_harness_session.rs:92` `start_sandboxed_cli_harness_session` | 285 → 286 | `complexity-svc-start-sandboxed-cli-harness-session-…md` |
| `tddy-coder/src/session_participant/terminal_manager.rs:105` `start_terminal` | 76 → 77 | none |
| `tddy-coding-agent-specs/src/headless.rs:227` `render` | 66 → 72 | none |
| `tddy-daemon-kernel/src/agent_tool_sockets.rs:272` `ensure_bound_blocking` | 66 → 77 | none |
| `tddy-daemon/src/runtime/build_context.rs:61` `new` | 65 → 71 | none |
| `tddy-daemon/src/runtime/mount_host.rs:227` `build_session_host` | 89 → 90 | none |
| `tddy-pty/src/runtime.rs:263` `open_pty_and_pump` | 179/9 → 188/9 | none |
| `tddy-session-lifecycle/src/connection_service/svc_host_builders.rs:33` `new` | 111 → 112 | none |
| `tddy-session-split/src/svc_spawn_split_agent.rs:237` `spawn_split_agent_process` | 130 → 132 | none |
| `tddy-supervisor/src/spawn_broker/child_environment.rs:51` `ChildEnvironment::build` | 70 → 78 | none |
| `tddy-supervisor/src/supervisor.rs:81` `Supervisor::new` | 71 → 80 | none |
| `tddy-terminal-rpc/src/local_pty_relay.rs:18` `run` | 140/8 → 141/8 | none |

The yolo rule counts each of these as reopened by this stack. Most grew by one to five lines; ten did so
under no record at all. See 6e Q1 for how to scope them.

`take_turn` (78/5) moved unchanged, so it is not reopened and stays out of scope by the rule. The
changeset lists it under *Not in scope*.

## 3. Seams

### `spec.rs` (1152 production lines; outline from `grep` of item heads, line spans by brace matching)

| Group | Items | Lines | Destination |
|---|---|---|---|
| headless section | `HeadlessSection`, `HeadlessPermissionPolicy`, `PermissionPolicyYaml`, `AsksCallerWord`, both `From` impls, `impl HeadlessSection` (852–977); `CodingAgentSpec::headless` (736–742) | ~133 | `spec::headless_section` |
| ACP section | `AcpSection`, `impl AcpSection`, `AcpCommandError`, `package_runner` (560–617); `CodingAgentSpec::{acp, acp_command, acp_mcp, acp_tool_deny}` (618–676) | ~117 | `spec::acp` |
| slot shapes | `McpSlot`, `McpFormat`, `ToolSlot`, `Unsupported`, `impl ToolSlot`, `FlagSlot`, `InstructionsSlot`, `HooksSlot` (979–1111) | ~133 | `spec::slot_shapes` |
| route + placement | `RouteSlot`, `ResumeSlot`, `Placement` + impls, `PlacementError` (762–851); `require_placement` (743–761); `route_slot_errors` (164–193); `routed_agent` (534–559) | ~165 | `spec::route_slot` |
| pass_env | `validate_env_entries`, `admits_a_tddy_variable`, `is_an_env_entry` (194–235); `CodingAgentSpec::{pass_env, passed_env}` (377–399) | ~65 | `spec::pass_env` |
| argv + models | `argv_params`, `binds_argv_param`, `resolved_tool`, `render_run_argv`, `model_listing_argv`, `curated_models` (408–533); `CuratedModel`, `ModelsSection` (1112–1152) | ~167 | `spec::argv` |
| stays | `CodingAgentExtension` + `validate`, `CodingAgentSpec`/`of`/`parse_section`, `SpecError`, `CodingAgentSection`, the capability accessors, `AgentVariant`/`variant_args` | ~370 | `spec.rs` |

The free items move with `move_item`. The `impl CodingAgentSpec` members move with `move_impl_members`
into the same destination modules. Each group is its own plan file, so no anchor is chained across a
moved group (the E1 defect in
`docs/dev/todo/2026-11-09-restructure-move-item-defects-found-splitting-coding-subagent-session.md`).
The crate re-exports from `lib.rs` (`pub use spec::…`), so `reexport: "outside"` keeps every consumer
path.

### `spawn_broker.rs` (984)

| Group | Lines | Movable by the engine? |
|---|---|---|
| plan types: `SpawnPlan`, `ChildOutput`, `EnvironmentBase`, `SESSION_PATH`, `FORWARDED_VARIABLES`, `SandboxJail`, `JailMount`, `PreExecStep`, `pre_exec_plan`, `privilege_to_drop`, `single_id_map`, `compile_bind_mount`, `unresolvable_mount_source` (84–321, 417–501) | ~320 | ✅ each item is defined once |
| process syscalls: `only_on_linux`, `set_parent_death_signal`×2, `lead_own_process_group`, `GroupCount`×2, `drop_privilege`, `change_directory`, `join_cgroup_scope` (322–416, 944–975) | ~125 | ❌ the `#[cfg(target_os)]` pairs are refused (`… names 2 items`) |
| resolve: `RESOLVE_*`×4, `open_without_following_symlinks`, `open_directory_reference`×2, `open_resolved`×2 (503–597) | ~95 | ❌ pairs |
| namespaces + mounts: the `/proc/self/*` consts, `enter_{user,mount,network}_namespace`×2, `write_small_file`, `make_root_mount_private`×2, `DESCRIPTOR_*`, `write_descriptor_path`, `bind_mount`×2, `bring_loopback_up`×2 (598–923) | ~325 | ❌ pairs |
| stays: module docs, `mod`/`use`, `ORPHANED_EXIT_STATUS`, `GroupListId`×2, `SUPERVISOR_SOCKET_ENV`, `USERS_FILE_ENV`, `path_to_c_string` | ~120 | — |

Moving only the plan types leaves the file at about 660 lines, still over budget. The syscall layer is
about 545 lines, too large for one destination, so it splits three ways. Each cfg pair has to become
one item first (6e Q2).

### `config.rs` (502 by the gate's awk / 507 by the record)

- `config::trust`: `require_trustworthy_config_file`, `require_trustworthy_config_directory`,
  `GROUP_OR_OTHER_WRITE` (419–493, about 75 lines). Callers: `SupervisorConfig::load` and
  `users_file.rs`.
- `config::users`: `UsersConfig`, `default_user_sockets_dir`, `refuse_old_user_list_keys` (51–64 and
  252–281, about 45 lines). These are not contiguous, so they need two `move_item` lines, the first one
  carrying `name`.

## 4. Non-measured backlog, verified

- **Dead module.** `packages/tddy-agent-launch/src/inherited_host_sockets.rs` (102 lines) is reached from:
  - `lib.rs:31` (`pub mod`);
  - `host_session_socket.rs:44/99/177` (`InheritedHostSocket`, the `inherited` map, `adopt_inherited`);
  - `tddy-session-lifecycle/src/connection_service.rs:294` (a re-export into the test module) and `:483`;
  - `inherited_host_sockets_tests.rs:11`.

  It has no production caller. The adoption tests are lines 89–278: `adopt_one`, the six `#[test]` and
  three `#[tokio::test]` adoption cases, and their helpers. The three merged-socket tests at 280–430 stay.
  `OwnerSocketKind::HOST_SESSION.setting` (`owner_socket.rs:44`) still reads `"host_sockets"`, which
  `owner_socket.rs:146` interpolates into a refusal.
- **Stale `allowed_env_keys` / `allowed_relay_tool_paths` wording**, with the intended reading of each hit:
  - `daemon.yaml.production` lines 42, 47 and 53;
  - `tddy-agent-launch/src/svc_ensure_project_available_for_start.rs:205`;
  - `tddy-session-lifecycle/src/coding_agent_launcher.rs:34`;
  - `tddy-session-lifecycle/tests/supervisor_spawn_delegation.rs` lines 224, 248 and 285;
  - `tddy-spawn/src/spawner.rs:626`;
  - `tddy-supervisor/proto/supervisor.proto:102`. This is a comment only, and `tonic-build` generates
    from it at build time, so no committed generated code changes;
  - `packages/tddy-supervisor/docs/architecture.md:294–296`, a package doc. It goes through the Final
    Checklist, not a direct edit.

  These hits are correct and stay: the load refusals and their tests (`config.rs:799–803`,
  `spawn_policy.rs:112–127`), the migration in `install`, `docs/ft/…`, and
  `coding_agent_relay.rs:352/376`, which asserts the old names are absent.
- **`TODO(sweep)`.** No code marker exists, here or on master. The intent is stated in #857's changeset:
  - Key the daemon's `toml_args` renderer off `RouteShape::TomlArgs` instead of the Codex-named
    `codex_acp_route_args`. That function is defined at `tddy-agent-backend/src/backend/routed.rs:30`,
    re-exported at `backend/mod.rs:15` and `lib.rs:35`, and reached as `tddy_core::codex_acp_route_args`
    from `tddy-session-lifecycle/src/coding_agent_launcher/acp.rs:51`. It is also used in
    `adapters/codex_acp_login.rs:18/48`. Docs name it in `tddy-agent-backend/docs/architecture.md:138`,
    `tddy-session-lifecycle/docs/coding-agent-launcher.md:48` and `specs/codex-acp.yaml:13`.
  - Widen `tests/launcher_names_no_agent.rs` past `"opencode"`. It currently scans for one string.
- **b8d9b769c's O_PATH descriptor and the handover slots.** `ForkBroker::start` calls
  `reserve_handover_slots()` before its thread exists. That call occupies fds 3..19 for the broker's
  lifetime, so any descriptor opened later, the checked O_PATH one included, lands at 19 or above. In
  the child, `hand_over_listeners` stages copies with `F_DUPFD_CLOEXEC` above `slots_end` and then
  `dup2`s onto the slots, so it replaces only slot fds. The collision cannot happen while the
  reservation holds. Nothing pins that invariant for the exec descriptor specifically. This is not a
  code issue (no measurement). 6e Q5 asks whether to add a pin.
- **`transport_of` / `permission_policy_of`.** The only matches are test helpers
  (`tddy-agent-backend/tests/spec_backend_unit.rs:41`, `tddy-coder/tests/spec_backend_build.rs:125`).
  No production wrapper remains.

## 5. After the 6e review (2026-11-10)

- **spawn_broker pairs as a group.** `anchors --items set_parent_death_signal` is refused ("names 2
  items"), so a cfg pair cannot be the anchor of a `move_item`. The plan instead uses a range
  `extract_module` over each contiguous run of pairs, written as a group:

  | Run | Lines | `check --deep` |
  |---|---|---|
  | process | 311–427 | ✅ |
  | resolve | 503–594 | 🔴 `no import rust-analyzer offered for path` |
  | namespaces | 596–918 | 🔴 the same |

  The two refusals happen because those runs are `#[cfg(target_os = "linux")]` code, which
  rust-analyzer on Darwin does not analyse.
- **Rename anchors.** An `items` anchor on `codex_acp_route_args` lands on its doc comment, where
  rust-analyzer gives no hover, and `--deep` refuses it. A range anchor on the identifier passes. The
  same holds for the `mod inherited_host_sockets_tests` rename.
- **users.yaml checks, side by side.**

  | Check | supervisor `from_yaml` | kernel `load_users_file` |
  |---|---|---|
  | shape (`deny_unknown_fields`) | ✅ | ✅ (`UserMapping` is `deny_unknown_fields`) |
  | empty `github_user` | refused | **accepted** |
  | `root` | refused | refused |
  | unsafe `os_user` | refused | **accepted** |
  | one login, two OS users | refused | refused |
  | > 15 OS users | refused | **accepted** |
  | identical repeated row | kept (`rows`) | deduplicated |
  | trust | root or own uid, file and every ancestor directory | root only, file only, supervised source only |

  The three **accepted** cells are the drift the shared crate closes.
