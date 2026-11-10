# Changeset: refactor — the `#agent-tools` sweep pays down every code issue the stack added

**Date**: 2026-11-10
**Status**: 🚧 In Progress — plan (commit 1); awaiting the 6e review
**Type**: Refactor
**Complexity**: 5/5
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
| `packages/tddy-supervisor/docs/code-issues/oversized-file-spawn-broker.md` | **fix here** (B2). Needs 6e Q2 |
| `packages/tddy-supervisor/docs/code-issues/oversized-file-config.md` | **fix here** (B3) |
| `packages/tddy-daemon-auth/docs/code-issues/oversized-file-auth.md` | out of scope: 579 → 579, the stack added only a row |
| `packages/tddy-coding-subagent/docs/code-issues/complexity-subagent-session-take-turn.md` | out of scope: moved unchanged (78/5). See 6e Q1 |
| `packages/tddy-agent-launch/docs/code-issues/complexity-*` (4 records the stack grew by 1 line) | reopened by the strict rule. Tier C, 6e Q1 |
| `docs/dev/todo/2026-11-09-dead-inherited-host-session-socket-adoption.md` | **✅ RESOLVED HERE** (B5) |
| other TODOs the stack added | the TODO gate, below |

No record is `Claimed by:` another open PR (`grep -rl 'Claimed by' packages/*/docs/code-issues/` over
the touched packages finds none in flight).

## Scope

- [ ] **Plan**: layout proposed, seam order fixed, plans anchored (`plans/`), the 6e answers folded in
- [ ] **Apply**: `check --deep` clean per plan, each plan applied and committed on its own, `verify --against` after each
- [ ] **Hand edits**: B2's cfg-pair merge (pending Q2), B5's deletion, B6's wording, the B7 test widening, each in its own commit
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

1. `spawn_broker::spawn_plan` (`move_item`, about 320 lines): `SpawnPlan`, `ChildOutput`,
   `EnvironmentBase`, `SESSION_PATH`, `FORWARDED_VARIABLES`, `SandboxJail`, `JailMount`,
   `PreExecStep`, `pre_exec_plan`, `privilege_to_drop`, `single_id_map`, `compile_bind_mount`,
   `unresolvable_mount_source`. `reexport: "outside"` keeps `tddy_supervisor::spawn_broker::SpawnPlan`
   and the others for the daemon's consumers.
2. **The cfg pairs become one item each.** This is a hand edit and is pending **6e Q2**. Each
   `#[cfg(target_os = "linux")] fn x` / `#[cfg(not(...))] fn x` pair becomes one `fn x` whose body
   carries the two `#[cfg]` blocks. The same applies to the `type` and `const` pairs, as one item with
   an inner cfg'd `const`/`type` or a `cfg_if`-free `#[cfg]` on the expression. No behaviour changes, and
   both targets compile as before. The engine refuses these items today: see the record's *Why the
   engine stops here*.
3. `move_item` the merged syscall layer into three siblings:
   - `spawn_broker::process_syscalls` (about 125 lines);
   - `spawn_broker::resolve` (about 95 lines);
   - `spawn_broker::namespaces` (about 325 lines).

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
| g | `tddy-daemon-kernel` `users_file.rs` `load_users_file` | 61/5 | extract the root-ownership check and the row checks (see 6e Q6 on the duplication) |
| h | `owner_socket.rs` `bind_owner_only` | 74/5 | extract the stale-socket removal branch |
| i | `tddy-cli-sessions` `pty_spawn.rs` `spawn_tool_on` | 104 | extract the env assembly and the command build |
| j | `tddy-coding-subagent` `session.rs` `opened` | CRAP 42 (c 6) | extract the hooks-scoped open (`begin_open` → `open_session` → `end_open` → merge) into `open_under_hooks` |
| k | `supervisor/users_change.rs` `apply_users_change` | c 7 | extract the added-users and removed-users halves |
| l | `server.rs` `resolve_spawn` | c 6 | extract the exec-root check from the env-rule check |
| m | `fork_broker.rs` `ChildSetup::run` | c 8 | extract the trailing `FD_CLOEXEC` clear (`keep_exec_descriptor_open`) and the handover call |
| n | `fork_broker.rs` `spawn_now` | c 6 | extract the stdio wiring |

Each one also gets a short "detected and closed" row in the change-history entry, since none has a
record. The sweep writes no record that it closes in the same PR.

### B4′ — Tier C: pre-existing functions this stack made worse (discovery §2c, 18 functions)

**Pending 6e Q1.** The yolo rule counts every row as reopened. Fourteen grew by 1–9 lines. Four grew
by 9 or more: `lib.rs run`, which is B4a, then `Supervisor::new` 71 → 80, `ChildEnvironment::build`
70 → 78, and `ensure_bound_blocking` 66 → 77.

The plan's default is the rule: extract each back under the budget. Six of them are 140–303-line
functions that already have records (`start_cli_harness_session` 303, `start_sandboxed_cli_harness_session`
286, `open_pty_and_pump` 188/9, …), so closing them is a large carve of `tddy-agent-launch`,
`tddy-pty` and `tddy-terminal-rpc` that the stack only brushed.

### B5 — `docs/dev/todo/2026-11-09-dead-inherited-host-session-socket-adoption.md` ✅ RESOLVED HERE

A hand deletion, which the user approved on 2026-11-09. It deletes:

- `packages/tddy-agent-launch/src/inherited_host_sockets.rs` and its `pub mod`;
- `HostSessionSockets::adopt_inherited` and the `inherited` map field;
- the re-export into the session-lifecycle test module (`connection_service.rs:294`);
- the adoption tests and helpers in `inherited_host_sockets_tests.rs` (lines 89–278), keeping the three
  merged-socket tests.

The remaining test module is renamed to `user_socket_tests` by `rename_symbol` (6e Q4).
`OwnerSocketKind::HOST_SESSION.setting` stops naming the removed `host_sockets:` key (6e Q4). The
TODO file is deleted in the same commit.

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

- `rename_symbol` `tddy_agent_backend::backend::routed::codex_acp_route_args` → `toml_args_route_args`
  (6e Q3). The engine rewrites the re-exports (`backend/mod.rs:15`, `lib.rs:35`), the
  `tddy_core::` path in `coding_agent_launcher/acp.rs:51`, and `adapters/codex_acp_login.rs`.
- `tests/launcher_names_no_agent.rs` scans for every shipped spec's tool id and product name, read from
  `tddy-coding-agent-specs/specs/*.yaml`, not just `"opencode"`. This widens the pin and does not change
  behaviour. It must be green after the rename. A failure means a launcher still names an agent, which
  is a finding to fix here, not a reason to narrow the test.

### Not in scope (verified, nothing to do)

- `transport_of` / `permission_policy_of`: only test helpers remain (discovery §4).
- b8d9b769c's O_PATH descriptor colliding with the handover slots: this cannot happen while
  `reserve_handover_slots` holds fds 3..19 from `ForkBroker::start` (discovery §4). Whether to pin it
  with a test is 6e Q5. It is not a TODO.
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
| `docs/dev/todo/2026-11-01-install-script-over-length-budget.md` | a bash split, which the Rust-only engine cannot do. It needs a sourcing convention and the `install_contract` checks updated | **continue as its own node**: the stack grew it 1,700 → 1,811, and it is stack-shared. A hand split by seam (account/identity, desktop, spawn-policy migration) is reviewable on its own. Or fold it into this node if the user prefers (6e Q8) |

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
| `09-spawn-broker-spawn-plan` | `move_item` (10 items, `name: spawn_plan`), widens `SESSION_PATH`, `FORWARDED_VARIABLES` → `pub(super)` | ✅ |
| `09-spawn-broker-spawn-plan-b` | `move_item` `single_id_map`, `compile_bind_mount`, `unresolvable_mount_source` → `spawn_broker::spawn_plan` | ⏳ needs 09 |

⚠ **Re-anchor before applying anything after 01.** Every applied plan rewrites `spec.rs`. Item anchors
survive edits outside their item, but the `<CodingAgentSpec>#N` indices in `01-b`, `02` and `04` count
impl blocks, so they shift as soon as an earlier plan moves one. A stale index is refused on
fingerprint, never mis-applied. `[G]` regenerates each later plan's anchor with
`restructure anchors … --items` right before it applies that plan. The module name `spawn_plan` (not
`pre_exec_plan`) is forced: a function `pre_exec_plan` already lives in `spawn_broker`, and the module
would collide with it (`E0428`, reported by `check --deep`).

Not yet planned as JSONL, because each depends on the tree that the plans above leave:

- B2 steps 2–3: the cfg-pair merge, pending Q2, then three `move_item`s;
- B3 is complete above;
- B4's `extract_method`s, which are `anchors --at` ranges taken after B1–B3 apply;
- B7's `rename_symbol`.

## Decisions & Trade-offs

- **One plan file per group**, not one plan for each file. This works around E1: a chained plan's
  anchor gets invalidated by an earlier move in the same file.
- **B2 needs a hand edit before the engine can move anything** (Q2). The alternative is teaching the
  engine to address a cfg-duplicated item by its predicate. That is real engine work, and it belongs in
  `tddy-code-restructuring`'s own backlog rather than this stack.
- **No record is written for the B4 findings.** The sweep both finds and closes them, so the
  change-history entry carries each one's before → after numbers instead.

## 6e questions for the user

1. **Tier C scope** (B4′, 18 functions, mostly +1–5 lines). Options:
   - (a) the strict rule: extract them all. This is a large carve of `tddy-agent-launch`, `tddy-pty`
     and `tddy-terminal-rpc`;
   - (b) close only the ones that grew by ≥ 5 lines or have no record, and append an "unchanged/+1"
     measurement row to the records of the rest;
   - (c) B4 only, with a row appended to each Tier C record.

   **Recommendation: (b).**
2. **`spawn_broker` cfg pairs** (B2 step 2). Either merge each `#[cfg]` pair into one item by hand (no
   behaviour change, both targets compile), or stop and fix the engine first. **Recommendation:
   merge.**
3. **The rename's name** (B7). `toml_args_route_args` (after `RouteShape::TomlArgs`), or
   `toml_route_overrides`?
4. **B5 details.** Rename `inherited_host_sockets_tests.rs` → `user_socket_tests.rs`? And
   `OwnerSocketKind::HOST_SESSION.setting`: drop the field for the embedded host (`setting: ""`, with the
   refusal no longer naming a supervisor key), or point it at `users.yaml`? **Recommendation: the
   rename, plus a refusal that names no supervisor key**, since an embedded host is never supervised.
   This changes one refusal string.
5. **O_PATH pin.** Add a unit test that a checked executable's descriptor opened after
   `ForkBroker::start` is never in `SD_LISTEN_FDS_START..+MAX_HANDED_OVER_LISTENERS`? Recommendation:
   yes. It is a pin of current behaviour, not a TODO.
6. **The users.yaml checks duplicated across crates** (`tddy-supervisor::users_file::from_yaml` and
   `tddy-daemon-kernel::users_file::load_users_file`). B4 c/g shrink both but keep two copies. Unifying
   them needs a shared crate (the supervisor must not depend on daemon code). Leave it, or add a node?
7. **`*_tests.rs` over 500.** `agent_activity_unit_tests.rs` 1293 → 1301 and three others. The gate's
   path filter does not exclude `_tests.rs`, but step 3.5 calls a test file over budget "not a
   decomposition". Treat them as out of scope? Recommendation: yes.
8. **`install`** (1,811 lines): its own node after the sweep, or folded into this one?
9. **Seam names in B1/B2/B3**: `spec::{headless_section, acp, slot_shapes, route_slot, pass_env,
   argv}`, `spawn_broker::{spawn_plan, process_syscalls, resolve, namespaces}`, `config::{trust,
   users}`. OK, or rename? (`headless_section` avoids a clash with the existing `crate::headless`
   renderer module.)

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
- [ ] `git rm` the three oversized-file records, the B5 TODO, and the reconciled rows
