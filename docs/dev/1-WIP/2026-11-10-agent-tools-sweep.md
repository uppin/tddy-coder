# Changeset: refactor — the `#agent-tools` sweep pays down every code issue the stack added

**Date**: 2026-11-10
**Status**: Green phase run in PR #876 (2026-11-10) — see the execution notes at the end.
Applied: spec split (01-06), config split (07-08), spawn_broker plans 11/12, the
`tddy-users-file` crate, the dead-module deletion, the wording pass, both renames (13, 14) and
the Tier C/B4 extracts. Deferred by the user: spawn_broker plans 09/10 to a Linux target.
**Type**: Refactor
**Complexity**: 5/5 (re-scored at the contract: it rose further with the new `tddy-users-file` crate and the 14 Tier C extracts; already at the ceiling)
**Stack**: `#agent-tools` 33/33, the sweep node. ⚠ **The stack merged while this node was being
planned.** #835 → #865 were squash-merged into `master` on 2026-11-10 at 04:24Z (`862aae64e` …
`65e6be1e2`). This branch is therefore cut from `origin/master` (`20f475576`) and its PR is based on
`master`, per `/add-to-pr-stack`'s merged-parent rule. Every package file is byte-identical to the top
node's head `b8d9b769c` (`git diff --stat b8d9b769c origin/master -- packages/` is empty), so the
measurements and anchors below hold on this base. The gh stack (#875) has no open member left to link
to, so this PR is not registered in it.

## Initial Discovery

Full exploration, the measurement commands and every trace:
[2026-11-10-agent-tools-sweep-initial-discovery.md](./2026-11-10-agent-tools-sweep-initial-discovery.md).

## Related Feature Documentation

None. This is a behaviour-preserving restructure, so it has no PRD.

## Summary

This node closes every code issue that the `#agent-tools` leftovers (#835 → #865) added or reopened.
That is three oversized files, the functions that crossed or worsened a length, nesting or CRAP budget,
and the dead inherited host-socket adoption the user approved deleting on 2026-11-09. It also fixes the
stale `allowed_env_keys` / `allowed_relay_tool_paths` wording and does #857's `TODO(sweep)` rename.

Every public path stays reachable. Crate-root re-exports and `reexport: "outside"` keep the consumers'
imports unchanged. The renamed route helper is the one exception: its callers are rewritten by the
engine's `rename_symbol`.

## Background

`/pr-wrap` step 3.5 declined to split `spec.rs`, `spawn_broker.rs` and `config.rs` mid-stack, because
each is stack-shared and a split would conflict through the diffs of the nodes above it. The stack has
now landed, so those splits are safe. The re-measurement in discovery §2 also turned up
function-level findings that no wrap recorded.

## Dependencies

- **Inherited:** every other `#agent-tools` node: n1–n19, #829–#834, and #835 → #836 → #857 → #858 →
  #859 → #865. All of them are merged into `master`, which is this node's base. This node implements nothing
  from any of them. It only moves, extracts, renames and deletes what they left.
- **Engine:** `tddy-tools restructure` on this branch (`packages/tddy-code-restructuring`). Its known
  `move_item` defects are in `docs/dev/todo/2026-11-09-restructure-move-item-defects-found-splitting-coding-subagent-session.md`.
  This plan works around E1 (the chained-anchor invalidation) by running one plan file per seam group.

## Prerequisites — the deferred-work cross-check (Step 2b)

| Record | Verdict |
|---|---|
| `packages/tddy-coding-agent-specs/docs/code-issues/oversized-file-spec.md` | **fix here** (B1) |
| `packages/tddy-supervisor/docs/code-issues/oversized-file-spawn-broker.md` | **fix here** (B2): cfg pairs moved as a group (user, 6e) |
| `packages/tddy-supervisor/docs/code-issues/oversized-file-config.md` | **fix here** (B3) |
| `packages/tddy-daemon-auth/docs/code-issues/oversized-file-auth.md` | out of scope: 579 → 579, the stack added only a row |
| `packages/tddy-coding-subagent/docs/code-issues/complexity-subagent-session-take-turn.md` | out of scope: moved unchanged (78/5) |
| `packages/tddy-agent-launch/docs/code-issues/complexity-*` (4 records the stack grew by 1 line) | Tier C: a measurement row only, per the user's option (b) (B4′) |
| `docs/dev/todo/2026-11-09-dead-inherited-host-session-socket-adoption.md` | **✅ RESOLVED HERE** (B5) |
| other TODOs the stack added | the TODO gate, below |

No record is `Claimed by:` another open PR (`grep -rl 'Claimed by' packages/*/docs/code-issues/` over
the touched packages finds none in flight).

## Scope

- [ ] **Plan**: layout proposed, seam order fixed, plans anchored (`plans/`), the 6e answers folded in
- [ ] **Apply**: `check --deep` clean per plan, each plan applied and committed on its own, `verify --against` after each
- [ ] **Hand edits**: B5's deletion and refusal, B6's wording, B8's crate body and consumer wiring, each in its own commit. No hand edit in B2
- [ ] **Baseline**: the scoped suites below, back to the recorded failing set by name
- [ ] **Code quality**: `cargo fmt --check`; `cargo clippy -p <touched> --all-targets -- -D warnings`
- [ ] **Re-measure**: `/analyze-code-issues` adds none and closes every backlog row. The records are deleted with their final numbers in the change-history entry
- [ ] **Documentation**: the Final Checklist executed

## Backlog — each item, and the intent that closes it

### B1 — `oversized-file-spec.md`: `spec.rs` 1152 → ≈ 370 production lines

Six `move_item` / `move_impl_members` groups into submodules of `spec`, one plan file each, in this order.
The free items go before the `impl CodingAgentSpec` members that name them.

1. `spec::headless_section`: `HeadlessSection`, `HeadlessPermissionPolicy`, `PermissionPolicyYaml`,
   `AsksCallerWord`, the two `From` impls, `impl HeadlessSection`, `CodingAgentSpec::headless`
   (about 133 lines).
2. `spec::acp`: `AcpSection`, its impl, `AcpCommandError`, `package_runner`, `CodingAgentSpec::{acp,
   acp_command, acp_mcp, acp_tool_deny}` (about 117 lines).
3. `spec::slot_shapes`: `McpSlot`, `McpFormat`, `ToolSlot`, `Unsupported`, `impl ToolSlot`,
   `FlagSlot`, `InstructionsSlot`, `HooksSlot` (about 133 lines).
4. `spec::route_slot`: `RouteSlot`, `ResumeSlot`, `Placement` with its impls, `PlacementError`,
   `CodingAgentSpec::require_placement`, `route_slot_errors`, `routed_agent` (about 165 lines).
5. `spec::pass_env`: `validate_env_entries`, `admits_a_tddy_variable`, `is_an_env_entry`,
   `CodingAgentSpec::{pass_env, passed_env}` (about 65 lines).
6. `spec::argv`: `CodingAgentSpec::{argv_params, binds_argv_param, resolved_tool, render_run_argv,
   model_listing_argv, curated_models}`, `CuratedModel`, `ModelsSection` (about 167 lines).

Closed when `spec.rs` ≤ 500 and no destination is > 500. Delete the record.

### B2 — `oversized-file-spawn-broker.md`: `spawn_broker.rs` 984 → ≈ 120

**The cfg pairs move as a group, never hand-merged** (user, 6e, 2026-11-10). `move_item` cannot
address a cfg-duplicated item at all: `anchors --items set_parent_death_signal` is refused with
"names 2 items" on this branch. Each contiguous run of pairs is therefore one `extract_module` over
the range that holds both definitions of every pair (`"group"`, `to_file: true`, `reexport: "glob"`),
so every pair moves as a unit into the same destination. The ranges are applied bottom-up, so the
ranges above stay valid. Each v1 plan is re-snapshotted after the one before it applies.

1. `09-spawn-broker-namespaces`: lines 596–918 → `spawn_broker::namespaces` (about 325 lines):
   `enter_{user,mount,network}_namespace`, `write_small_file`, `make_root_mount_private`,
   `DESCRIPTOR_*`, `write_descriptor_path`, `bind_mount`, `bring_loopback_up`, and the
   `/proc/self/*` consts.
2. `10-spawn-broker-resolve`: lines 503–594 → `spawn_broker::resolve` (about 95 lines): the
   `RESOLVE_*` pairs, `open_without_following_symlinks`, `open_directory_reference`, `open_resolved`.
3. `11-spawn-broker-process-syscalls`: lines 311–427 → `spawn_broker::process_syscalls` (about 120
   lines): `only_on_linux`, `set_parent_death_signal`, `lead_own_process_group`, `GroupCount`,
   `drop_privilege`, `change_directory`. Then `-b` moves `join_cgroup_scope` (a single item) into it.
4. `12-spawn-broker-spawn-plan` (`move_item`, about 320 lines): `SpawnPlan`, `ChildOutput`,
   `EnvironmentBase`, `SESSION_PATH`, `FORWARDED_VARIABLES`, `SandboxJail`, `JailMount`,
   `PreExecStep`, `pre_exec_plan`, `privilege_to_drop`. Then `-b` moves `single_id_map`,
   `compile_bind_mount` and `unresolvable_mount_source`. `reexport: "outside"` keeps
   `tddy_supervisor::spawn_broker::SpawnPlan` and the others for the daemon's consumers.

⚠ **At plan time, on this macOS host, the engine runs step 3 and refuses steps 1 and 2.** `check --deep`
on step 1 or step 2 refuses the whole group: *"no import rust-analyzer offered for `path` left fewer of
its unresolved occurrence(s)"*. Those regions are mostly `#[cfg(target_os = "linux")]`, and
rust-analyzer on Darwin does not analyse cfg'd-out code, so their names do not resolve. Step 3 passes
`--deep` (5 widenings to `pub(crate)`).

**Per the user, `[G]` does not hand-edit around this.** It first re-runs `check --deep` for 09/10 on a
Linux host, or with rust-analyzer's `cargo.target` set to a Linux triple. If the group is still
refused there, it **stops and asks**.

Closed when every file in `spawn_broker/` is ≤ 500. Delete the record.

### B3 — `oversized-file-config.md`: `config.rs` 502 → ≈ 380

- `config::trust` (`move_item`, `name`): `require_trustworthy_config_file`,
  `require_trustworthy_config_directory`, `GROUP_OR_OTHER_WRITE`.
- `config::users`: `UsersConfig`, `default_user_sockets_dir` (`move_item` with `name`), then
  `refuse_old_user_list_keys` (a second `move_item` into it).

Delete the record.

### B4 — functions: new findings this stack added (discovery §2b)

Each one gets `extract_method`(s) until it is ≤ 60 lines, brace depth ≤ 4 and cyclomatic complexity
≤ 5 (CRAP ≤ 30 at 0 % coverage). Ranges are anchored with `anchors --at` after B1–B3 have applied.
The extracts are ordered bottom-up within each function. No range holds an early `return`.

| # | Function | Now | Intent |
|---|---|---|---|
| a | `tddy-supervisor` `lib.rs` `run` | 111 lines, c 12 | extract the users-watch start, the user-socket survey and the shutdown wait as three helpers |
| b | `users_watch.rs` `UsersWatch::spawn` | 83/5 | extract the notify callback body into `fn on_users_event(…)`, and the bounded reload into `fn reload_settled(path)` |
| c | `users_file.rs` `UsersFile::from_yaml` | 63/6 | extract the per-row refusal (`fn refuse_row`) and the ambiguous-mapping check (`fn refuse_two_os_users`) |
| d | `client.rs` `open_event_stream` | 66 | extract the first-event read (`first_event`) and the forwarding task body (`forward_frames`) |
| e | `streamed.rs` `for_each_line` | depth 6 | extract the per-chunk byte loop (`split_chunk`) |
| f | `streamed.rs` `forward_lines_until_exit` | depth 5 | extract the `Some(line)` arm (`forward_line`) |
| g | `tddy-daemon-kernel` `users_file.rs` `load_users_file` | 61/5 | the row checks go to `tddy-users-file` (B8). Extract the root-ownership check |
| h | `owner_socket.rs` `bind_owner_only` | 74/5 | extract the stale-socket removal branch |
| i | `tddy-cli-sessions` `pty_spawn.rs` `spawn_tool_on` | 104 | extract the env assembly and the command build |
| j | `tddy-coding-subagent` `session.rs` `opened` | CRAP 42 (c 6) | extract the hooks-scoped open (`begin_open` → `open_session` → `end_open` → merge) into `open_under_hooks` |
| k | `supervisor/users_change.rs` `apply_users_change` | c 7 | extract the added-users and removed-users halves |
| l | `server.rs` `resolve_spawn` | c 6 | extract the exec-root check from the env-rule check |
| m | `fork_broker.rs` `ChildSetup::run` | c 8 | extract the trailing `FD_CLOEXEC` clear (`keep_exec_descriptor_open`) and the handover call |
| n | `fork_broker.rs` `spawn_now` | c 6 | extract the stdio wiring |

Each one also gets a short "detected and closed" row in the change-history entry, since none has a
record. The sweep writes no record that it closes in the same PR.

### B4′ — Tier C: pre-existing functions this stack made worse (discovery §2c)

**Scope (user, 6e, 2026-11-10: option (b)).** Extract a function when the stack grew it by 5 or more
lines, or when it has no record. Otherwise, add a fresh measurement row to its record and do not
extract it.

**Extracted** (14), each brought to ≤ 60 lines and depth ≤ 4, anchored with `anchors --at` after
B1–B3 apply:

| Function | base → head | Why it is in scope |
|---|---|---|
| `tddy-agent-launch` `host_session_socket.rs` `ensure_bound` | 65 → 75 | +10, no record |
| `tddy-agent-launch` `opencode_cli_spawn/harness_launch.rs` `launch` | 69 → 74 | +5, no record |
| `tddy-agent-launch` `svc_signal_delete_session.rs` `delete_session_as` | 90 → 94 | no record |
| `tddy-coder` `session_participant/terminal_manager.rs` `start_terminal` | 76 → 77 | no record |
| `tddy-coding-agent-specs` `headless.rs` `render` | 66 → 72 | +6, no record |
| `tddy-daemon-kernel` `agent_tool_sockets.rs` `ensure_bound_blocking` | 66 → 77 | +11, no record |
| `tddy-daemon` `runtime/build_context.rs` `new` | 65 → 71 | +6, no record |
| `tddy-daemon` `runtime/mount_host.rs` `build_session_host` | 89 → 90 | no record |
| `tddy-pty` `runtime.rs` `open_pty_and_pump` | 179/9 → 188/9 | +9, no record — the largest |
| `tddy-session-lifecycle` `connection_service/svc_host_builders.rs` `new` | 111 → 112 | no record |
| `tddy-session-split` `svc_spawn_split_agent.rs` `spawn_split_agent_process` | 130 → 132 | no record |
| `tddy-supervisor` `spawn_broker/child_environment.rs` `ChildEnvironment::build` | 70 → 78 | +8, no record |
| `tddy-supervisor` `supervisor.rs` `Supervisor::new` | 71 → 80 | +9, no record |
| `tddy-terminal-rpc` `local_pty_relay.rs` `run` | 140/8 → 141/8 | no record |

**Measurement row added in this commit, not extracted** (4). Each one is +1 under an existing record:

- `packages/tddy-agent-launch/docs/code-issues/complexity-opencode-cli-spawn-start-cli-harness-session.md` (302 → 303)
- `packages/tddy-agent-launch/docs/code-issues/complexity-opencode-cli-spawn-resume-resume-cli-harness-session.md` (117 → 118)
- `packages/tddy-agent-launch/docs/code-issues/complexity-svc-resume-sandboxed-cli-harness-session-relaunch-sandboxed-cli-harness.md` (139 → 140)
- `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-sandboxed-cli-harness-session-start-sandboxed-cli-harness-session.md` (285 → 286)

### B5 — `docs/dev/todo/2026-11-09-dead-inherited-host-session-socket-adoption.md` ✅ RESOLVED HERE

A hand deletion, which the user approved on 2026-11-09. It deletes:

- `packages/tddy-agent-launch/src/inherited_host_sockets.rs` and its `pub mod`;
- `HostSessionSockets::adopt_inherited` and the `inherited` map field;
- the re-export into the session-lifecycle test module (`connection_service.rs:294`);
- the adoption tests and helpers in `inherited_host_sockets_tests.rs` (lines 89–278), keeping the three
  merged-socket tests.

Then:

- `14-session-lifecycle-user-socket-tests` (`rename_symbol`, passes `--deep`) renames the test module to
  `user_socket_tests` (user, 6e). `[G]` confirms the file moved with the module, since rust-analyzer
  renames a module's file.
- **The refusal names no supervisor key** (user, 6e). `bind_owner_only`'s not-privileged closure
  becomes `owner_socket::not_privileged_to_hand_over(kind, euid, os_user, target_uid)`: one wording
  for both kinds, telling the operator to map the user in `users.yaml`. The `OwnerSocketKind::setting`
  field, now read by nothing, is deleted. This changes a refusal string, pinned red by
  `owner_socket::tests`.
- The TODO file is deleted in the same commit.

### B6 — stale wording: `allowed_env_keys` / `allowed_relay_tool_paths`

Comments and the production config template only, so behaviour is unchanged:

- `daemon.yaml.production` lines 42, 47 and 53 change to the `exec_roots` and whole-env reading;
- `svc_ensure_project_available_for_start.rs:205`;
- `coding_agent_launcher.rs:34`;
- `supervisor_spawn_delegation.rs` lines 224, 248 and 285;
- `spawner.rs:626`;
- `supervisor.proto:102`.

The package-doc hit, `tddy-supervisor/docs/architecture.md:294–296`, is in the Final Checklist.

### B7 — #857's `TODO(sweep)`: the route renderer keyed off its shape

- `13-agent-backend-toml-route-overrides`: `rename_symbol` `tddy_agent_backend::backend::routed::codex_acp_route_args`
  → `toml_route_overrides` (user, 6e). It passes `--deep`. The engine rewrites the re-exports (`backend/mod.rs:15`, `lib.rs:35`), the
  `tddy_core::` path in `coding_agent_launcher/acp.rs:51`, and `adapters/codex_acp_login.rs`.
- `tests/launcher_names_no_agent.rs::the_daemon_acp_launcher_names_no_shipped_agent` (added in the
  contract) scans the launcher for every shipped spec's id segments and product, read from
  `tddy-coding-agent-specs/specs/*.yaml`. It matches whole identifier segments. It is red today on
  exactly one line, `acp.rs:51 … tddy_core::codex_acp_route_args(route)`, and the rename turns it
  green.

### B8 — the users.yaml model and validation, shared (user, 6e, 2026-11-10)

Two copies of the users.yaml row checks had drifted:

- `tddy-supervisor::users_file::UsersFile::from_yaml` (B4c) checks shape, no root, no empty login,
  a safe `os_user`, one OS user per login, and at most 15 OS users.
- `tddy-daemon-kernel::users_file::load_users_file` (B4g) checks only root and the one-OS-user rule.
  It accepts an `os_user` that cannot name a socket, more OS users than the supervisor can hand over,
  and a row with no login.

A new workspace crate, **`packages/tddy-users-file`**, holds the model (`MappedUser`, `UsersMapping`,
`MAX_MAPPED_OS_USERS`, `is_one_safe_segment`) and the one validation, `UsersMapping::from_yaml`, which
returns a `UsersFileRefusal`.

- **Dependencies:** `serde` and `serde_yaml` only, both of which its two consumers already use. No
  external dependency, so `Cargo.lock` gains only the crate itself.
- **Supervisor:** `UsersFile` wraps `UsersMapping` and keeps reading only `os_users()`. The crate names
  no coding agent and no daemon type.
- **Trust check:** not shared. The readers trust different owners. The supervisor accepts root or its
  own uid and walks every directory above the file (`config::trust`, B3). A supervised daemon accepts
  root alone, for the file only. The only common part is a one-line mode mask, so each reader keeps its
  own check (decision below).
- **Behaviour change, daemon only:** the daemon now refuses the three mappings above, the same ones
  the supervisor already refuses. Pinned red by the kernel tests in the contract.
- **Kernel code:** `load_users_file` maps `UsersMapping::distinct_rows()` into its
  `config::access::UserMapping`, and its row checks and hand-rolled dedup go. That also closes B4g.

The contract commit adds the crate with its API surface stubbed (`unimplemented!`, marked
`TODO(#agent-tools 33/33 green)`) and its tests, the path dependency in both consumers, and the
consumer-side tests.

### Not in scope (verified, nothing to do)

- `transport_of` / `permission_policy_of`: only test helpers remain (discovery §4).
- b8d9b769c's O_PATH descriptor colliding with the handover slots: this cannot happen while
  `reserve_handover_slots` holds fds 3–18 from `ForkBroker::start` (discovery §4). It is not a TODO.
  It is now pinned by `handover::tests::a_descriptor_opened_while_the_slots_are_reserved_lands_above_every_slot`
  (user, 6e), which is green.
- `oversized-file-auth.md` (579 unchanged) and `complexity-subagent-session-take-turn.md` (moved
  unchanged).

## For the TODO gate (not restructurable, so not in the plan)

| TODO | Why no restructure closes it | Recommendation |
|---|---|---|
| `docs/dev/todo/2026-11-07-setpriv-acp-agent-keeps-daemon-user-logname.md` | a bug fix (behaviour change): `wrap_for_user` must set `USER`/`LOGNAME` | **continue**: a small fix node on top of the sweep, one override plus one unit test |
| `docs/dev/todo/2026-11-08-install-renders-configured-harness-exec-roots.md` | feature work plus an open design question (how `install` learns the configured agents while the supervisor names none) | **defer**: needs a design decision first. The operator workaround is documented |
| `docs/dev/todo/2026-11-09-daemon-codex-acp-login-runs-as-the-daemon-user-in-the-worktree.md` | behaviour change: the login has to be spawned through the user switch, plus a per-conversation directory the session user owns | **defer**: its own feature node with an acceptance test. It is not a sweep-sized fix |
| `docs/dev/todo/2026-11-09-daemon-creates-no-cgroup-scope-for-supervised-spawns.md` | feature work | already **deferred** by the user (2026-11-09). Not re-asked |
| `docs/dev/todo/2026-11-09-restructure-move-item-defects-found-splitting-coding-subagent-session.md` | engine work in `tddy-code-restructuring`, not this stack's code | **defer**: this plan works around E1 (one plan per group), and E2 is caught by the compile gate. Continue only if B1–B3 hit it in a way the workaround cannot absorb |
| `docs/dev/todo/2026-11-01-install-script-over-length-budget.md` | a bash split, which the Rust-only engine cannot do. It needs a sourcing convention and the `install_contract` checks updated | **deferred by the user** (6e, 2026-11-10). The consent line is recorded in the entry by this commit |

## Callers

| | |
|---|---|
| B1 | consumers of `tddy_coding_agent_specs::{HeadlessSection, AcpSection, McpSlot, Placement, …}` go through `lib.rs` re-exports, and `reexport: "outside"` re-points in-crate paths. Expect no consumer edits. `check --deep` lists any it finds |
| B2 | `tddy_supervisor::spawn_broker::{SpawnPlan, EnvironmentBase, ChildOutput, PreExecStep, …}` is used by `server.rs`, `fork_broker.rs`, `child_environment.rs`, the daemon's `tddy-spawn`, and the supervisor tests. `reexport: "outside"` keeps the outside paths |
| B3 | `crate::config::require_trustworthy_config_*` is used by `users_file.rs`, and `crate::config::UsersConfig` by `supervisor.rs`/`lib.rs`. `reexport: "outside"` |
| B5 | `tddy-agent-launch`, `tddy-session-lifecycle` (test module) |
| B7 | `tddy-agent-backend`, `tddy-core` (re-export), `tddy-session-lifecycle` |

## Visibility

Filled at apply time from each run's `visibility:` lines. B2's syscall moves will widen the private
`unsafe fn`s to `pub(super)`. Each one is listed here with its safety doc checked.

## Baseline

Recorded before any change, scoped (`./test -p <pkg>`, `--no-fail-fast`, `--test-threads=1`), with
`CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0`, on `b8d9b769c`:

`./test -p tddy-coding-agent-specs -p tddy-supervisor -p tddy-agent-launch -p tddy-daemon-kernel -p tddy-spawn -p tddy-cli-sessions -p tddy-agent-backend -p tddy-session-lifecycle`

| Run | Before (2026-11-10, `b8d9b769c`; `packages/` identical to master `20f475576`) | After |
|---|---|---|
| 8 packages, 211 test binaries | **2302 passed · 9 failed · 1 ignored**; 4 targets failed, all in `tddy-session-lifecycle` | |

The 9 failures are pre-existing and environmental. The after-run must fail with exactly these names,
for the same reasons:

| Target | Test | Why (this host) |
|---|---|---|
| `claude_cli_from_spec_acceptance` | `an_unsandboxed_claude_cli_start_runs_the_spec_binary_with_the_argv_its_template_renders` | argv assertion at `:486`. The binary path is under the nix-shell `TMPDIR` (`/private/tmp/nix-shell.*`): the known macOS TMPDIR noise |
| `cursor_cli_from_spec_acceptance` | `jailed::a_jailed_cursor_sessions_hook_reaches_the_daemon_through_the_bridge` | macOS jail bridge, the known `sandbox RPC bridge not installed` harness gap |
| `session_room_acceptance` | `starting_a_session_puts_no_terminal_bridge_in_the_lobby` | LiveKit testkit container: Docker `failed to bind host port … address already in use` |
| `session_sync_livekit_acceptance` | `follows_the_session_head_when_the_agent_commits`, `mirrors_a_file_the_agent_wrote_without_a_commit`, `mirrors_an_edit_to_an_existing_file`, `mirrors_binary_content_byte_for_byte`, `removes_a_file_the_agent_deleted`, `restores_a_mirror_that_was_corrupted_by_hand` | `tddy-remote-git-repo is not built at target/debug`: `./test` does not build it. Harness, not code |

Every other target is green: `tddy-coding-agent-specs`, `tddy-supervisor` (on macOS; its Linux-only
e2e cases run on CI), `tddy-agent-launch`, `tddy-daemon-kernel`, `tddy-spawn`, `tddy-cli-sessions`,
`tddy-agent-backend`, and the rest of `tddy-session-lifecycle`.

Packages B4/B4′ reach that are not in the run above are `tddy-coding-subagent`, `tddy-daemon`,
`tddy-pty`, `tddy-terminal-rpc`, `tddy-session-split` and `tddy-coder`. Each one is baselined by
`[G]` before its first plan touches it, and only if 6e keeps Tier C.

Contract-commit run of the new tests (`./dev cargo test -p tddy-users-file`; `-p tddy-daemon-kernel --lib
users_file owner_socket::tests`; `-p tddy-supervisor --lib the_shared_mapping_budget
a_descriptor_opened_while_the_slots`; `-p tddy-session-lifecycle --test launcher_names_no_agent`):

- tddy-users-file: 0 passed, 11 failed;
- kernel: 4 passed, 5 failed (the 4 existing users_file tests are green);
- supervisor: 2 passed;
- launcher_names_no_agent: 1 passed, 1 failed.

Each is exactly as the *Draft PR contract* table says. `cargo check --all-targets` over those four
packages is clean.

| Gate | Before | After |
|---|---|---|
| `cargo fmt --check` | not run at plan time (the restructure apply rustfmts what it writes) | |
| `cargo clippy -p <touched> --all-targets -- -D warnings` | run by `[G]` before the first plan | |

## Plan

`docs/dev/1-WIP/2026-11-10-agent-tools-sweep-plans/`, one JSONL per seam group, applied in filename
order, each in its own commit:

There is one operation per file. An operation that moves into a module the previous file created is a
`-b`/`-c` file of its own, because a deep check resolves every line against the unchanged tree and
cannot see a module that an earlier line of the same plan creates.

| Plan | Op | `check --deep` at plan time |
|---|---|---|
| `01-spec-headless-section` | `move_item` (7 items, `name: headless_section`) | ✅ no findings |
| `01-spec-headless-section-b` | `move_item` `<CodingAgentSpec>#5` → `spec::headless_section` | ⏳ needs 01 applied (the module does not exist yet) |
| `02-spec-acp` | `move_item` (5 items incl. `<CodingAgentSpec>#3`, `name: acp`) | ✅ |
| `03-spec-slot-shapes` | `move_item` (8 items, `name: slot_shapes`) | ✅ |
| `04-spec-route-slot` | `move_item` (7 items incl. `<CodingAgentSpec>#6`, `name: route_slot`) | ✅ |
| `04-spec-route-slot-b` / `-c` | `move_item` `route_slot_errors` / `routed_agent` → `spec::route_slot` | ⏳ needs 04 |
| `05-spec-pass-env` | `move_item` (3 items, `name: pass_env`), widens `admits_a_tddy_variable` → `pub(super)` | ✅ |
| `05-spec-pass-env-b` | `move_impl_members` `CodingAgentSpec::{pass_env, passed_env}` → `spec::pass_env` | ⏳ needs 05 |
| `06-spec-argv` | `move_impl_members` (6 members, `name: argv`) | ✅ |
| `06-spec-argv-b` | `move_item` `CuratedModel`, `<CuratedModel>`, `ModelsSection` → `spec::argv` | ⏳ needs 06 |
| `07-config-trust` | `move_item` (3 items, `name: trust`) | ✅ |
| `08-config-users` | `move_item` `UsersConfig`, `default_user_sockets_dir` (`name: users`) | ✅ |
| `08-config-users-b` | `move_item` `refuse_old_user_list_keys` → `config::users` | ⏳ needs 08 |
| `09-spawn-broker-namespaces` | `extract_module` over the range 596–918 (every pair in it, as one `group`), `to_file` → `spawn_broker::namespaces` | 🔴 refused on macOS: cfg'd-out names unresolved (B2). Retry on a Linux target, then stop and ask |
| `10-spawn-broker-resolve` | `extract_module` over the range 503–594, `group` → `spawn_broker::resolve` | 🔴 refused on macOS, the same way |
| `11-spawn-broker-process-syscalls` | `extract_module` over the range 311–427, `group` → `spawn_broker::process_syscalls`, widens 5 fns → `pub(crate)` | ✅ |
| `11-spawn-broker-process-syscalls-b` | `move_item` `join_cgroup_scope` → `spawn_broker::process_syscalls` | ⏳ needs 11 |
| `12-spawn-broker-spawn-plan` | `move_item` (10 items, `name: spawn_plan`), widens `SESSION_PATH`, `FORWARDED_VARIABLES` → `pub(super)` | ✅ |
| `12-spawn-broker-spawn-plan-b` | `move_item` `single_id_map`, `compile_bind_mount`, `unresolvable_mount_source` → `spawn_broker::spawn_plan` | ⏳ needs 12 |
| `13-agent-backend-toml-route-overrides` | `rename_symbol` `codex_acp_route_args` → `toml_route_overrides` (range anchor on the name) | ✅ |
| `14-session-lifecycle-user-socket-tests` | `rename_symbol` `mod inherited_host_sockets_tests` → `user_socket_tests` (after B5's deletion) | ✅ |

09–11 are v1 range plans, applied bottom-up (09 → 10 → 11) so each range above stays at its line.
Re-run `restructure snapshot` on the next one after each apply.

⚠ **Re-anchor before applying anything after 01.** Every applied plan rewrites `spec.rs`. Item anchors
survive edits outside their item, but the `<CodingAgentSpec>#N` indices in `01-b`, `02` and `04` count
impl blocks, so they shift as soon as an earlier plan moves one. A stale index is refused on
fingerprint, never mis-applied. `[G]` regenerates each later plan's anchor with
`restructure anchors … --items` right before it applies that plan. The module name `spawn_plan` (not
`pre_exec_plan`) is forced: a function `pre_exec_plan` already lives in `spawn_broker`, and the module
would collide with it (`E0428`, reported by `check --deep`).

Not yet planned as JSONL, because each depends on the tree that the plans above leave:

- B4/B4′'s `extract_method`s: `anchors --at` ranges, taken after B1–B3 apply;
- B8's crate body and consumer wiring: hand-written green code, because the crate is new and the
  engine moves only existing code. `is_one_safe_segment` and the checks are moved from the
  supervisor by hand, since the engine cannot move code into a crate that is not yet in the graph.
  `[G]` may instead try `move_item` into the stub crate once it compiles.

## Decisions & Trade-offs

- **One plan file per operation.** This works around E1, where a chained plan's anchor is invalidated
  by an earlier move in the same file. It also lets a deep check, which resolves every line against the
  unchanged tree, prove each creating move on its own.
- **No record is written for the B4 findings.** The sweep both finds and closes them, so the
  change-history entry carries each one's before → after numbers instead.
- **2026-11-10, user, 6e: Tier C is option (b).** Extract the functions grown by 5 or more lines, or
  with no record (14). The other 4 get a fresh measurement row only (B4′).
- **2026-11-10, user, 6e: the spawn_broker cfg pairs are moved as a group, never hand-merged.** If the
  engine cannot run a grouped plan for a pair, `[G]` stops and asks. Planned as one range
  `extract_module` per contiguous run of pairs, since `move_item` cannot anchor a cfg-duplicated item
  (B2). Steps 1–2 are refused on this macOS host (cfg'd-out code). `[G]` retries on a Linux target,
  then stops and asks.
- **2026-11-10, user, 6e: the install split is deferred**, and the consent is recorded in its TODO.
  It is not in this node.
- **2026-11-10, user, 6e: the users.yaml duplication is resolved here** with a new crate,
  `tddy-users-file` (B8). No external dependency, and nothing agent- or daemon-specific reaches the
  supervisor. **Trust check not shared**: the two readers trust different owners and scopes (B8),
  and a shared function would take its policy as parameters while saving one mode mask.
- **2026-11-10, user, 6e: the route helper is renamed `toml_route_overrides`** (B7).
- **2026-11-10, user, 6e: the dead-module refusal names `users.yaml` and no supervisor key.** The test
  module is renamed `user_socket_tests` (B5).
- **2026-11-10, user, 6e: the O_PATH pin test is added** (green; it pins current behaviour).
- **2026-11-10, user, 6e: test files over 500 lines are out of scope.**
- **2026-11-10, user, 6e: seam names as planned:** `spec::{headless_section, acp, slot_shapes,
  route_slot, pass_env, argv}`, `spawn_broker::{spawn_plan, process_syscalls, resolve, namespaces}`,
  `config::{trust, users}`.

## Draft PR contract

What this PR owns, and what `[G]` must leave true:

- **New crate `tddy-users-file`** (public API, stubbed in the contract):
  - `MAX_MAPPED_OS_USERS: usize = 15`
  - `MappedUser { github_user, os_user }` (serde, `deny_unknown_fields`)
  - `UsersMapping::{from_yaml, rows, distinct_rows, os_users}`
  - `UsersFileRefusal::{Malformed, NoGithubUser, MapsToRoot, UnsafeOsUser, TwoOsUsers, TooManyOsUsers}` + `Display` (the supervisor's current wording, no file prefix)
  - `is_one_safe_segment`
- **`tddy_daemon_kernel::owner_socket::not_privileged_to_hand_over(kind, euid, os_user, target_uid) -> String`**
  (stubbed). `OwnerSocketKind::setting` is deleted at green.
- **Renamed:** `tddy_agent_backend::toml_route_overrides`, which was `codex_acp_route_args`, and its
  `tddy_core` re-export.
- **Moved, with paths kept by re-exports:** the B1/B2/B3 modules listed above.
- **Deleted:** `tddy_agent_launch::inherited_host_sockets`, `HostSessionSockets::adopt_inherited`.

Tests in the contract (red pins new behaviour; green pins current behaviour that must stay):

| Test | State at contract |
|---|---|
| `tddy-users-file` `tests::blank_text_is_an_empty_mapping` | 🔴 (stub) |
| `tests::reads_each_row_in_order_and_the_distinct_os_users_it_names` | 🔴 |
| `tests::an_identical_repeated_row_is_one_distinct_row` | 🔴 |
| `tests::refuses_text_that_is_not_a_list_of_rows` | 🔴 |
| `tests::refuses_a_row_with_no_github_user` | 🔴 |
| `tests::refuses_a_mapping_to_root` | 🔴 |
| `tests::refuses_an_os_user_that_cannot_name_a_socket` | 🔴 |
| `tests::refuses_one_login_mapped_to_two_os_users` | 🔴 |
| `tests::admits_fifteen_distinct_os_users_and_refuses_sixteen` | 🔴 |
| `tests::a_refusal_reads_as_the_supervisor_words_it_today` | 🔴 |
| `tests::a_safe_segment_is_plain_ascii_and_not_hidden` | 🔴 |
| `tddy-daemon-kernel` `users_file::tests::a_daemon_refuses_a_users_yaml_whose_os_user_cannot_name_a_socket` | 🔴 (new daemon refusal) |
| `users_file::tests::a_daemon_refuses_a_users_yaml_naming_more_os_users_than_the_supervisor_can_hand_over` | 🔴 |
| `users_file::tests::a_daemon_refuses_a_users_yaml_row_that_names_no_github_user` | 🔴 |
| `owner_socket::tests::a_refusal_to_hand_over_a_host_session_socket_names_users_yaml_and_no_supervisor_key` | 🔴 (stub; the renamed refusal) |
| `owner_socket::tests::both_socket_kinds_give_the_same_remedy` | 🔴 |
| `tddy-supervisor` `users_file::tests::the_shared_mapping_budget_is_every_handover_slot_but_the_daemons_own` | 🟢 |
| `handover::tests::a_descriptor_opened_while_the_slots_are_reserved_lands_above_every_slot` | 🟢 (O_PATH pin) |
| `tddy-session-lifecycle` `launcher_names_no_agent::the_daemon_acp_launcher_names_no_shipped_agent` | 🔴 (one line: `codex_acp_route_args` in `acp.rs:51`) |

The existing users.yaml suites stay green throughout: the supervisor's `users_file::tests` and the
kernel's four `users_file::tests` (green at the contract). So does the recorded baseline, with the same
9 failing names.

## 6e questions for the user

All answered by the user on 2026-11-10. See *Decisions & Trade-offs*.

## Final Checklist

- [ ] `packages/tddy-coding-agent-specs/docs/changesets/2026-11-10-agent-tools-sweep.md`: spec.rs split, with the before/after numbers
- [ ] `packages/tddy-supervisor/docs/changesets/2026-11-10-agent-tools-sweep.md`: spawn_broker/config splits, the B4 functions
- [ ] `packages/tddy-agent-launch/docs/changesets/…`: the deleted adoption module
- [ ] `packages/tddy-agent-launch/docs/module-layout.md:368`: drop `inherited_host_sockets.rs` from the row
- [ ] `packages/tddy-session-lifecycle/docs/session-identity.md:216`: the test file's new name
- [ ] `packages/tddy-supervisor/docs/host-sockets.md:88`: the test file's new name
- [ ] `packages/tddy-supervisor/docs/architecture.md:294–296`: `allowed_env_keys` no longer gates anything; it says the spawn's env passes whole except loader variables
- [ ] `packages/tddy-supervisor/docs/architecture.md`: the module map names `spawn_broker/{spawn_plan,process_syscalls,resolve,namespaces}.rs` and `config/{trust,users}.rs`
- [ ] `packages/tddy-coding-agent-specs` docs (module map, if one lists `spec.rs` contents): the `spec/` submodules
- [ ] `packages/tddy-agent-backend/docs/architecture.md:138`, `packages/tddy-session-lifecycle/docs/coding-agent-launcher.md:48`, `specs/codex-acp.yaml:13`: the renamed helper
- [ ] `docs/dev/changesets/2026-11-10-agent-tools-sweep.md`: the change-history entry, with every closed record's final measurement
- [ ] `packages/tddy-users-file/README.md` and a `docs/` changeset entry: the new crate. Also the supervisor and daemon-kernel docs that describe users.yaml validation (`packages/tddy-supervisor/docs/architecture.md`, `docs/ft/supervisor/tddy-supervisor.md`) name the shared crate and the daemon's new refusals
- [ ] `git rm` the three oversized-file records and the B5 TODO

## Execution notes (2026-11-10, #876)

- Plans 09/10 not applied: deferred by the user, 2026-11-10 (#876 6e). `spawn_broker.rs` stands
  at 534 production lines; the record is narrowed and the follow-up is
  `docs/dev/todo/2026-11-10-spawn-broker-linux-only-moves.md`.
- Plan 14's engine rename renamed the module declaration but did not move its file (E0583); the
  file was moved to `user_socket_tests.rs` as the noted post-move completion, and verify with
  `--repoint inherited_host_sockets_tests=user_socket_tests` accounts for every statement.
- Engine-blocked Tier C/B4 seams (each tried at least twice): `build_session_host` (daemon) and
  `DaemonSessionHost::new` are builder composition roots whose chain/alias seams name
  `SplitRoomHosting`/`TerminalExposure`, which the files do not import; `spawn_tool_on`
  (cli-sessions) names the exit hook's `Output`, likewise unimported; the streamed
  `for_each_line`/`forward_lines_until_exit` ranges hold early `return`s the engine refuses.
  Functions left at 61 lines by a signature-inclusive count: `delete_session_as`,
  `spawn_split_agent_process`.
- Three plan-only commits in the stack (`117cda382`, `72700a22f`, `ce545ab48`) hold re-anchored
  or refused-attempt plan files; the code for their groups landed in the following commits.
- Verification (2026-11-10): scoped `./test` batches — users-file+supervisor+kernel+specs+
  agent-backend 953 passed/0 failed; agent-launch+pty+terminal-rpc+session-split+cli-sessions+
  coding-subagent+spawn 663/0; daemon 313/0; coder+session-lifecycle 1175 passed/2 failed, the
  two failures being the recorded pre-existing `an_unsandboxed_claude_cli_start…` and
  `jailed::…hook_reaches_the_daemon…`. Clippy (`./dev cargo clippy … -D warnings`) clean for all
  fifteen touched packages; `cargo fmt --check` clean. Linux-only tests
  (`supervisor_user_sockets`, the Linux cfg ranges in spawn_broker) compile-checked on macOS and
  run on CI only.

## Round-two verification (2026-11-10)

- Further extracts: `delete_session_as` 61 → 52, `spawn_split_agent_process` 61 → 59,
  `spawn_tool_on` 92 → 79, `DaemonSessionHost::new` 101 → 97 (default socket/rosters fields),
  `build_session_host` 82 → 80 (`daemon_launcher_deps`); `for_each_line`'s emitter is now the
  free `emit_trimmed_line` (still depth >4: the per-chunk loop's `return` is refused on the
  third attempt). Remaining over-budget, all engine-blocked as documented above:
  `build_session_host` (80), `DaemonSessionHost::new` (97), `spawn_tool_on` (79),
  `for_each_line` / `forward_lines_until_exit` depth.
- Scoped tests: agent-launch+session-split+supervisor+cli-sessions 659/0; daemon 312 passed/1
  failed (`reads_a_remote_agents_files_from_its_own_clone`, a Docker host-port collision; it
  passes alone, 1/1); session-lifecycle 939 passed/2 failed, the two failures being the
  recorded pre-existing `an_unsandboxed_claude_cli_start…` and
  `jailed::…hook_reaches_the_daemon…`. Combined with the earlier batches (953/0, 663/0, daemon
  313/0, coder+lifecycle 1175/2), no failure is attributable to this sweep.
- Clippy (`./dev cargo clippy` over all 15 touched packages, `-D warnings`) and
  `cargo fmt --check` clean. Linux-only tests compile-checked on macOS and run on CI only.
