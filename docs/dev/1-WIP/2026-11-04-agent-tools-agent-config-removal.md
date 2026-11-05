# Changeset: Agent settings come only from specs; the daemon.yaml agent types and readers are deleted

**Created:** 2026-11-04
**Status:** 🚧 In Progress
**Type:** Feature (breaking configuration change)
**Complexity**: 5 (re-scored at red, unchanged; grew in scope: +`tddy-actions`, `tddy-sandbox-recipes`, `tddy-sandbox-app` for the `home:` path form, and the agentless ACP path deletion in `coding_agent_launcher.rs`) — several row-4 signals: crates `tddy-daemon-kernel`, `tddy-agent-launch`, `tddy-session-lifecycle`, `tddy-session-activity`, `tddy-session-split`, `tddy-daemon-rpc`, `tddy-telegram-control`, `tddy-daemon`, `tddy-coding-agent-specs`; a breaking config-type removal; an answer-wait hold whose value must match what the agent was launched with (ordering); ~40 fixture files migrated; `coding_agents` becomes non-optional
**Stack:** `#agent-tools 19/19` — branch `feature/agent-tools/agent-config-removal`, base `feature/agent-tools/telegram-spec-launch` (#824)
**PR:** [#825](https://github.com/uppin/tddy-coder/pull/825) (draft)
**PRD:** none of its own. Completes the "daemon.yaml agent sections removed" criterion of the registry PRD on `feature/agent-tools/agent-registry` (`docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`)

## Initial Discovery

[2026-11-04-agent-tools-agent-config-removal-initial-discovery.md](./2026-11-04-agent-tools-agent-config-removal-initial-discovery.md)

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-04-removed-agent-config-readers-remain.md](../todo/2026-11-04-removed-agent-config-readers-remain.md) | ✅ **RESOLVED HERE.** Fields, types and readers deleted; `daemon_config_from_fixture_file` deleted; Telegram's delete gets the registry |
| [2026-11-04-telegram-delete-leaves-agent-worktrees.md](../todo/2026-11-04-telegram-delete-leaves-agent-worktrees.md) | ✅ **RESOLVED HERE** (the file exists at this base). The delete passes the registry #824 gave the controller; delete the record at wrap |
| [2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md](../todo/2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md) | ✅ **RESOLVED HERE** (settled at red, see Decisions). `coding_agents` becomes non-optional in `DaemonCodingAgentLauncher` too, so the agentless hand-built ACP path is deleted (§ Removed) and its fixtures move to `with_coding_agents`; delete the record at wrap |
| [2026-11-03-harness-clis-legacy-cli-harness-removal.md](../todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md) | ⚠ During. `cli_harness_binary` and `CliHarnessConfig` go here; `svc_terminal_ports.rs:268` moves off the legacy `CliHarness` + `cli_harness_binary` onto the spec. The `CliHarness` table itself stays (sweep) |
| `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-coding-agent-launcher.md`, `packages/tddy-telegram-control/docs/code-issues/oversized-file-telegram-session-control-chaining-and-listing.md` | ⚠ During. Touched at the binary reads and the delete wiring; no split (sweep). Re-measure at wrap |

## Affected Packages

- [ ] [`tddy-daemon-kernel`](../../../packages/tddy-daemon-kernel/README.md): `DaemonConfig.{claude_cli, cursor_cli, cli_harnesses}`, `ClaudeCliConfig`, `CursorCliConfig`, `CliHarnessConfig` and their readers deleted; `refuse_removed_agent_keys` kept
- [ ] [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): binary, tddy-tools and hook-url readers move here (spec + host); binary resolvers refuse instead of returning the configured value as-is; `LaunchSessions.coding_agents` non-optional
- [ ] [`tddy-session-activity`](../../../packages/tddy-session-activity/README.md): `AnswerWaitPorts` holds per agent
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): `coding_agent_launcher.rs` binary reads, `svc_activity_ports.rs`, `svc_terminal_ports.rs`; ~34 fixture files; `test_util::daemon_config_from_fixture_file` deleted
- [ ] [`tddy-session-split`](../../../packages/tddy-session-split/README.md): hook url, answer wait, claude binary reads
- [ ] [`tddy-daemon-rpc`](../../../packages/tddy-daemon-rpc/README.md): `catalog/ports.rs` model probe; 2 fixtures
- [ ] [`tddy-telegram-control`](../../../packages/tddy-telegram-control/README.md): the delete passes the registry; test struct literals
- [ ] [`tddy-daemon`](../../../packages/tddy-daemon/README.md): 4 fixtures; Telegram wiring passes the registry
- [ ] [`tddy-coding-agent-specs`](../../../packages/tddy-coding-agent-specs/README.md): `claude.yaml` `run.env: { CLAUDE_CODE_NO_FLICKER: "1" }`
- [ ] [`tddy-actions`](../../../packages/tddy-actions/README.md): `HomeSection::Path { path }` — the path form of a tool's `home:` (settled at red); a relative path refused at load
- [ ] [`tddy-sandbox-recipes`](../../../packages/tddy-sandbox-recipes/README.md), `tddy-sandbox-app`: `lower_home` / the banner of a path home

## Responsibility

- **Delete** the daemon-kernel `claude_cli`, `cursor_cli`, `cli_harnesses` fields (`config.rs:155,158,162`), the three types (`agent_cli.rs:148-188,248-262`) and their readers (`resolve_claude_binary_path`, `claude_cli_answer_wait_secs`, `claude_cli_renderer_env`, `cli_harness_binary`, `resolve_cursor_binary_path`, `resolve_cursor_cli_tddy_tools_path`, `resolve_cursor_cli_daemon_url`, `resolve_cursor_home_dir`, `resolve_claude_home_dir`, `daemon_hook_urls::claude_hook_daemon_url`). `refuse_removed_agent_keys` stays: a daemon.yaml naming them still fails start-up with the replacement named.
- **Readers move up to the launch crates**, because the kernel has no edge to specs: the binary is `HostCodingAgents::resolve(id, …)` (spec `binary.env`, then host auto-resolution in `DaemonToolHost::located` without the config); tddy-tools from `tools/tddy-tools.yaml`/the host; the hook url from `local_daemon_hook_url`/`listen.advertise_url`.
- **Binary resolvers' "as-is" last step becomes a refusal** (`resolve_claude_binary_path` step 4, `cli_harness_binary`'s `default_binary`): a binary not found is refused naming the agent and its spec's `binary.env`.
- **`coding_agent.answer_wait_secs` is wired to its callers**: the hook parameters at launch (`claude_hooks.rs:51,118-120` writes `timeout = wait + margin`) read the spec value, and the **server-side hold is per agent and matches the hook timeout written at launch** — the wait is recorded in the session directory at start (`tddy_session_activity::answer_wait::record_answer_wait`, file `answer-wait-ms` beside `.session.yaml`) and `AwaitQuestionAnswer` holds for `session_answer_wait(session_dir, session_type, agents)`, never re-reading a since-edited override. A session with no record uses its agent spec's `answer_wait_secs`, then the specs' `DEFAULT_ANSWER_WAIT_SECS` — never a daemon-wide value. `AnswerWaitPorts.wait` is deleted.
- **`claude.yaml` declares `run.env: { CLAUDE_CODE_NO_FLICKER: "1" }`**, and the out-of-band env additions are removed (`claude_cli_renderer_env`, `svc_start_sandboxed_claude_cli_session.rs:394`, `svc_relaunch_sandboxed_runner.rs:158` — wherever they survive #823); the jailed process gets it through `run.env`.
- **Fixtures:** the ~40 callers of `daemon_config_from_fixture_file` (session-lifecycle 34, daemon 4, daemon-rpc 2) move onto `test_config()`/`DaemonConfig::load` with a fixture host mapping each spec's `binary.env` to a stub (`test_util::coding_agents_with_binaries`), and the function is deleted. Telegram test struct literals drop the fields.
- **`LaunchSessions.coding_agents` becomes non-optional** (`launch_ports.rs:130`): every launcher has coding agents; the "no coding agents → `FailedPrecondition`" refusals become construction-time guarantees where the type allows.
- **Telegram delete removes an agent session's worktree again**: `chaining_and_listing.rs:412-420` passes the registry #824 gave the controller to `delete_session_directory`, so `worktree_removal_applies_to` follows the spec's `KeepWorktreeOnDelete`.
- **`claude_home_dir` keeps its feature** (settled at red): `home: { kind: path, path: <dir> }` (`HomeSection::Path`) is an operator's own absolute directory, mounted read-write; the refusal of daemon.yaml `claude_cli.claude_home_dir` names that form (`removed_agent_keys.rs` `claude_move`). `cursor_home_dir` keeps `home.key` (not part of the decision).
- **`coding_agents` is non-optional everywhere** (settled at red), `DaemonCodingAgentLauncher` included: its agentless hand-built ACP path is deleted.
- **Changelog entry** for the breaking change in `docs/ft/daemon/changelog/` (removed keys, their spec replacements, the binary refusal, `claude_home_dir` → the `home:` path form, hook urls → `listen.advertise_url`, the route-shape renames from #819).

## Boundaries

- Does not touch the `CliHarness` table or `SupervisorClient::call`'s deadline — sweep node. (The agentless ACP launcher path **is** this node's now — settled at red.)
- Does not change any spec semantics beyond `claude.yaml`'s `run.env`, the `home:` path form, and, if needed, `answer_wait_secs: 120` declared explicitly.
- No proto or web change. No file splits (code-issue records only).

**Stack-wide rules this node keeps:**
- The agent process sees no `TDDY_*` variable: the readers removed here were the last daemon-config-to-agent env path; a test asserts a claude-cli start's environment has no `TDDY_` key.
- No production fallback without coding agents (now a type guarantee); no "as-is" binary fallback.
- No hangs: the answer hold is bounded by the per-session recorded wait.
- Fixtures install coding agents and the sandbox RPC bridge; Linux jail tests self-skip without user namespaces.
- Deletions not listed in **Removed** need the developer's approval.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n16` spec-backend (#822) | no workflow backend built from an agent-id match | the kernel config has no backend-side reader left | change `SpecBackend` |
| `n18` telegram-spec-launch (#824) | Telegram launches through the spec path, reads no `claude_cli`/`cursor_cli`, and holds the host's coding agents | the delete uses that handle; the fields can go | change Telegram's start or pickers |
| `n17` cli-dispatch-collapse (#823) | the bespoke claude/cursor launch code (and most of its config reads) deleted | only the remaining readers are moved here | re-add any launch path |
| `n12` agent-registry (#801) | `refuse_removed_agent_keys`, `CodingAgentSpec::answer_wait_secs`, `Capability::KeepWorktreeOnDelete` | kept; wired; used by the delete | change the refusal text — **except** `claude_home_dir`'s replacement, which names the `home:` path form by the settled decision |

**Parent-green needs found at red (6e/6f).** Each test below fails today for this node's reason (a `TODO(agent-config-removal)` body, or the daemon-wide / out-of-band behaviour this node removes), but runs through a parent's launch, which must be green first. The real objects are kept because the integration *is* the point:

| Test | Needs green | Why |
|---|---|---|
| `yaml_only_agent_acceptance::an_override_of_claude_yaml_with_answer_wait_secs_…`, `::editing_answer_wait_secs_after_a_start_…` | n17 (#823), n14 (#820) | a host claude-cli start through the generic spec launch, its hooks file from the `claude-settings-local` slot. Starts already succeed at this base; the failure is the 120 s daemon-wide hold and the `120 + 10` hook timeout |
| `claude_cli_fullscreen_renderer::*` (host start, resume, jail) | n17 (#823) | the start/resume/relaunch paths whose out-of-band env this node removes are #823's to collapse first; the failure today is `CLAUDE_CODE_NO_FLICKER=1` reaching an override that dropped it. (The three "turned off" cases were already red at the base — n12's accepted regression, the spec launch ignoring `claude_cli.fullscreen: false`; migrated, they are red for this node's reason) |
| `claude_cli_session_acceptance::a_claude_cli_start_whose_binary_is_not_found_…` | n17 (#823) | the start resolves `claude` through `HostCodingAgents::resolve`; fails today on the `daemon_tool_host` TODO |
| `telegram_session_control_integration::a_telegram_delete_of_a_claude_cli_session_…` | n18 (#824) — only `with_coding_agents`, already implemented at this base | fails today because the delete passes `None` |
| `no_agent_name_gates::the_fixtures_start_every_agent_from_a_daemon_yaml_with_no_agent_section` | none | 55 files still set or read a removed section at this base; it goes green with the fixture migration |

Units (`answer_wait_unit`, `daemon_tool_host_unit`, `lower_jail_sections_unit::a_path_home_…`, `tool_registry_acceptance::a_path_home_that_is_not_absolute_…`, `removed_agent_keys_unit::a_claude_cli_section_…`) need no parent: they fail only on this node's TODOs.

**Inherited red recorded at 6c (not this node's):** the parents' contract tests in `tddy-agent-launch` (lib: `cli_agent_steps`, `harness_from_spec`, `start_request_checks`, `session_acting_identity` — 26), `tddy-coding-agent-specs` (`cli_spec_features_unit`, `harness_specs_acceptance`, `headless_protocol_spec_unit`, `route_body_drift_acceptance`, `routed_agent_unit`), `tddy-telegram-control` (`telegram_agent_choices_unit`, `telegram_branch_conflict_acceptance`, `telegram_reads_no_agent_config`, `telegram_start_agent_from_spec`), `tddy-session-lifecycle` (106 of 928 failed in 24 targets: lib, `agent_terminal_recording_launch_acceptance`, `claude_cli_from_spec_acceptance`, `claude_cli_fullscreen_renderer` (the three "turned off" cases), `claude_cli_permission_mode_acceptance`, `claude_cli_session_acceptance` (`…metadata_fields_persisted`, `…livekit_fields_empty`, `…resume_relaunches_in_worktree`, `start_session_claude_cli_threads_initial_prompt_from_request`), `claude_cli_tmux_reattach_acceptance`, `claude_cli_tmux_session_acceptance`, `cli_spec_features_acceptance`, `harness_clis_from_spec_acceptance`, `host_without_integrations`, `list_sessions_managed_codebase_acceptance`, `managed_codebase_workflow_acceptance`, `no_agent_name_gates` (#823's agent-id scan), `operator_session_starter_unit`, `sandbox_behavior_acceptance`, `session_branch_conflict_acceptance`, `session_room_acceptance`, `ssh_exec_start_acceptance`, `start_cancel_acceptance`, `start_phase_acceptance`, `tool_session_host_socket`, `warp_cli_session`, `yaml_only_agent_acceptance` (9)), `tddy-daemon` (`provider_route_listing_acceptance`, `remote_managed_worktree_cross_host_acceptance`, `split_session_resume_acceptance`, `test_placement`) and `tddy-daemon-rpc` (`project_home_session_tools_acceptance`; the local run hit its 50-minute bound inside `remote_codebase_clone_placement_acceptance`, so the suites after it were left to CI). Causes: n13–n18 TODO bodies, and `builtin_coding_agents` fixtures failing "tool tddy-tools is unavailable" from the pre-fix n12 head. The baseline ran on the base plus this node's surface-only edits, which change no behaviour (claude.yaml's `run.env` sets the value the out-of-band addition already sets).

## Draft PR contract

Published by the contract commit (unimplemented bodies are `// TODO(agent-config-removal): implement`; no field is deleted in it):

- `tddy-session-activity/src/answer_wait.rs`: `ANSWER_WAIT_FILE`, `record_answer_wait(session_dir, wait)`, `recorded_answer_wait(session_dir)`, `session_answer_wait(session_dir, session_type, agents)` (TODO). `AnswerWaitPorts.wait` and the hold in `question_answers.rs` are marked for green.
- `tddy-agent-launch/src/host_coding_agents.rs`: `daemon_tool_host(base: Arc<dyn ToolHost>) -> Arc<dyn ToolHost>` (TODO) — the daemon's host with no daemon.yaml input, whose refusal is the spec resolution's `ToolError::Unresolvable` naming the agent, its `binary.env` and every place searched (no new error type: that refusal already says it). `DaemonToolHost`, `claude_cli_hooks` (answer wait, tddy-tools, hook url), the four out-of-band `CLAUDE_CODE_NO_FLICKER` additions and `LaunchSessions.coding_agents` are marked.
- `tddy-actions/src/tool/home.rs`: `HomeSection::Path { path }` (settled decision); its arms in `tddy-sandbox-recipes::lower_home`, `tddy-agent-launch::jail_home` and `tddy-sandbox-app`'s banner are TODO; `ToolDef::parse`'s relative-path refusal is marked.
- `tddy-coding-agent-specs/specs/claude.yaml`: `run.env: { CLAUDE_CODE_NO_FLICKER: "1" }` — live (today the out-of-band addition sets the same value, so nothing changes until it is removed).
- `tddy-session-lifecycle/src/test_util.rs`: `coding_agents_with_binaries(&[(id, binary)])` — implemented (fixture infrastructure: the built-in agents on a host mapping each spec's `binary.env`, with an empty `PATH`). `DaemonCodingAgentLauncher.coding_agents` is marked.
- `tddy-telegram-control`: the delete's `None` is marked with the fix (`Some(&self.coding_agents)`, #824's handle — no new symbol needed).
- `tddy-daemon-kernel`: `claude_move`'s `claude_home_dir` text is marked.
- Test support: `agent_registry_support::{builtin_yaml_run_as_the_stub_waiting, ARegistryDaemon::{restart_with_tool, an_orchestrator_of, asks_for_a_permission, the_permission_hook_of, permission_hook_timeout_of}, AHeldHook, PERMISSION_REQUEST_ID}`; `claude_cli_fullscreen_renderer`'s fixture rewritten off `claude_cli:` onto a `<tddyhome>/tools/claude.yaml` override.

## Green wave

**Wave:** 5 of 5
**Greenable independently:** after #822 and #824 are green — the fields can be deleted only once no backend (#822) and no Telegram path (#824) reads them
**Concurrent with:** none
**Blocks:** nothing in this stack (the reserved sweep node, n20, follows)

Real dependency edges, as opposed to the branch line:

    n12 → n13      n13 → n14, n15, n16      n15 → n16      n14 → n17      n17 → n18      n16, n18 → n19

## State A

- `DaemonConfig.{claude_cli, cursor_cli, cli_harnesses}` still exist (`config.rs:155-162`); `DaemonConfig::load` refuses them, but `serde_yaml::from_str` and struct literals still set them.
- 28 production read sites in 19 files outside the kernel (discovery §1.3), fewer after #823 and #824.
- `answer_wait_secs` comes from `claude_cli.answer_wait_secs`; the server hold is one daemon-wide `Duration` (`svc_activity_ports.rs:103` → `question_answers.rs:47`).
- `claude.yaml` has `run.env: {}` (:29); `CLAUDE_CODE_NO_FLICKER` is added by `claude_cli_renderer_env` outside the spec.
- `daemon_config_from_fixture_file` (`test_util.rs:58-70`) has 40 caller files.
- Telegram's delete passes `None` (`chaining_and_listing.rs:412-420`), leaving every agent worktree behind.

## State B

- No agent setting is read from daemon.yaml anywhere; the kernel has no agent type.
- Each agent's answer hold equals the hook timeout it was launched with.
- A claude-cli session's no-flicker env comes from `claude.yaml` and an override can drop it.
- A Telegram delete removes an agent session's worktree unless its spec keeps it.

## Delta

### New
- `docs/ft/daemon/changelog/2026-11-04-agent-tools-daemon-yaml-agent-sections-removed.md` (at wrap)
- `packages/tddy-session-lifecycle/src/test_util.rs` `coding_agents_with_binaries`
### Modified
- `tddy-daemon-kernel`: `config.rs`, `config/agent_cli.rs` (keepers only: `find_real_harness_on_host`, `resolve_harness_binary_name`, `choose_*_binary`), `config/daemon_hook_urls.rs`
- `tddy-agent-launch`: `host_coding_agents.rs`, `hooks_and_urls.rs`, `notify_env.rs`, `harness_from_spec.rs` (`configured_cli_binary`), `cli_harness_jail.rs`, `launch_ports.rs`, plus any reader #823 left
- `tddy-session-activity/src/service/question_answers.rs`; `tddy-session-lifecycle/src/connection_service/{svc_activity_ports,svc_terminal_ports}.rs`, `coding_agent_launcher.rs`
- `tddy-session-split/src/{svc_spawn_split_agent,service_util}.rs`; `tddy-daemon-rpc/src/catalog/ports.rs`
- `tddy-telegram-control/src/telegram_session_control/chaining_and_listing.rs`; `tddy-daemon/src/runtime/telegram_wiring.rs`
- `tddy-coding-agent-specs/specs/claude.yaml`
- Fixtures: the 40 files in discovery §1.6; `tddy-telegram-control/tests/{telegram_branch_conflict_acceptance,telegram_start_claude_acceptance,telegram_start_cursor_acceptance}.rs` literals (if #824 left any); `claude_cli_fullscreen_renderer.rs:176`, `unified_worktree_base_acceptance.rs:73`
- Docs at wrap: `docs/ft/daemon/{claude,cursor,opencode,muse,warp}-cli-session.md`, `session-agent-roster.md`, `telegram-session-control.md`, `packages/tddy-daemon-kernel/README.md`, `packages/tddy-coding-agent-specs/docs/coding-agent-specs.md` (`run.env`, `answer_wait_secs`), `config.example.yaml`, `dev.daemon.yaml:130`
### Removed
- `DaemonConfig.{claude_cli, cursor_cli, cli_harnesses}` and their `Default` entries and re-exports (`config.rs:313-318`)
- `ClaudeCliConfig`, `CursorCliConfig`, `CliHarnessConfig`
- `resolve_claude_binary_path`, `claude_cli_answer_wait_secs`, `claude_cli_renderer_env`, `cli_harness_binary`, `resolve_cursor_binary_path`, `resolve_cursor_cli_tddy_tools_path`, `resolve_cursor_cli_daemon_url`, `resolve_cursor_home_dir`, `resolve_claude_home_dir`, `daemon_hook_urls::claude_hook_daemon_url`, `CLAUDE_FULLSCREEN_ENV` once unused
- `configured_binary` (`spec_launch.rs:475`, if #823 left it), `configured_cli_binary` and its `TODO(agent-registry)`, `configured_cursor_binary`
- The kernel's `DEFAULT_ANSWER_WAIT_SECS` (the specs' stays)
- `tddy_session_lifecycle::test_util::daemon_config_from_fixture_file` and its `TODO(agent-config-removal)`
- `DaemonToolHost` and its `config` (`host_coding_agents.rs`) — replaced by `daemon_tool_host`
- `AnswerWaitPorts.wait` (and its `Default` value) — the hold is per session
- The agentless hand-built ACP launch path of `DaemonCodingAgentLauncher` (settled at red): the `Option` around its `coding_agents`, `coding_agents_error`'s hand-built fallback branch, and the items `2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md` lists (`harness_command`, `apply_route`, the ACP constants, `claude_agent_acp_command`, `opencode_route_config`, `opencode_withdrawal_config`, `CLAUDE_NATIVE_TOOLS`, `OPENCODE_NATIVE_TOOLS`, `ACP_HARNESSES`, `withdrawal_session_meta`) and the hand-built `mcp_servers_wrapped` registration (`TODO(claude-headless)`)
- The out-of-band `CLAUDE_CODE_NO_FLICKER` additions: `claude_cli_spawn.rs:280`, `svc_resume_claude_cli_session.rs:307`, `svc_start_sandboxed_claude_cli_session.rs:394`, `svc_relaunch_sandboxed_runner.rs:158` (wherever #823 left them)
- `claude_cli_session_acceptance.rs::a_host_without_coding_agents_refuses_a_claude_cli_and_a_cursor_cli_start_failed_precondition_spawning_nothing` (#823's test) — a host without coding agents can no longer be built; the refusal becomes a type guarantee. Its `a_host_without_coding_agents` fixture stays as the base this node's binary-refusal test adds agents to, renamed at green
- `docs/dev/todo/2026-11-04-telegram-delete-leaves-agent-worktrees.md`, `2026-11-04-removed-agent-config-readers-remain.md`, `2026-11-04-acp-hand-built-launch-path-for-agentless-launchers.md` (at wrap)
- Kernel tests `config.rs:993 claude_cli_config_tests`, `:1218 cursor_cli_config_tests`, `tests/answer_wait_secs_config.rs`
- The `TODO(agent-registry)` at `chaining_and_listing.rs:412-420`
- The `Option` around `LaunchSessions.coding_agents`

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/yaml_only_agent_acceptance.rs`** (`agent_registry_support`, `a_hook_reports_running` :330)

Written at red (2026-11-05); every one fails today for this node's reason (observed failure in brackets).

- [x] `an_override_of_claude_yaml_with_answer_wait_secs_holds_a_permission_hook_that_long_and_writes_that_timeout_into_the_hooks_file` (:540) [still held at the 8 s budget; hooks-file timeout 130, not 12]
- [x] `editing_answer_wait_secs_after_a_start_does_not_change_that_sessions_hold` (:575) [same: held 8 s; timeout 130]

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/claude_cli_fullscreen_renderer.rs`** (fixture rewritten off `claude_cli:` onto a claude.yaml override)

- [x] `a_claude_cli_start_has_claude_code_no_flicker_from_claude_yaml_and_an_override_without_it_does_not` (:674) [the override's start shows `CLAUDE_CODE_NO_FLICKER=1`]
- [x] `jailed::a_jailed_claude_cli_start_has_claude_code_no_flicker_from_claude_yaml_and_an_override_without_it_does_not` (:719, macOS only; widened from the planned positive-only name, which passes today) [same, in the jail]
- [x] Migrated, now red for the same reason: `a_host_session_starts_claude_inline_when_an_override_of_claude_yaml_drops_no_flicker` (:428), `resuming_a_host_session_relaunches_claude_inline_when_an_override_of_claude_yaml_drops_no_flicker` (:485), `a_sandboxed_session_starts_claude_inline_when_an_override_of_claude_yaml_drops_no_flicker` (:551); the three fullscreen-by-default cases stay green

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/claude_cli_session_acceptance.rs`**

- [x] `a_claude_cli_start_whose_binary_is_not_found_is_refused_naming_its_binary_env` (:811) [`daemon_tool_host` TODO panic]

**`tddy-telegram-control`: `packages/tddy-telegram-control/tests/telegram_session_control_integration.rs`**

- [x] `a_telegram_delete_of_a_claude_cli_session_removes_its_worktree_and_of_a_cursor_cli_session_keeps_it` (:789) [the claude-cli worktree is left behind]

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/no_agent_name_gates.rs`** (sibling scan)

- [x] `the_fixtures_start_every_agent_from_a_daemon_yaml_with_no_agent_section` (:436) [55 files set or read a removed section]
- [x] `the_removed_agent_config_scan_finds_keys_types_fields_and_the_fixture_reader_and_skips_comments` (:412) — the scanner's own shape test; passes

**`tddy-daemon-kernel`: `packages/tddy-daemon-kernel/tests/removed_agent_keys_unit.rs`** (settled decision)

- [x] `a_claude_cli_section_is_refused_naming_claude_yaml_and_each_field_it_set` (:27) — expectation updated: `claude_home_dir → home: { kind: path, path: <dir> }` [still names `home.key`]

Kept green: `tddy-daemon/tests/removed_agent_config_acceptance.rs`, the rest of `removed_agent_keys_unit.rs`.

### Unit / integration tests

- [x] `tddy-session-activity` `tests/answer_wait_unit.rs`: `a_recorded_wait_reads_back_as_recorded` (:64), `a_session_that_recorded_no_wait_reads_back_none` (:82), `the_recorded_wait_is_the_hold_even_when_the_spec_now_says_otherwise` (:94), `a_session_with_no_record_is_held_for_its_agent_specs_answer_wait` (:108), `a_session_of_a_type_no_spec_names_is_held_for_the_specs_default` (:121) [TODO panics]
- [x] `tddy-agent-launch` `tests/daemon_tool_host_unit.rs`: `claude_is_found_in_the_hosts_local_bin_with_no_daemon_config` (:125), `a_wrapper_shim_directory_on_the_hosts_path_is_passed_over_for_the_real_claude` (:137), `a_claude_found_nowhere_is_refused_naming_its_binary_env_and_every_place_searched` (:151) [TODO panic]
- [x] `tddy-actions` `tests/tool_registry_acceptance.rs`: `a_path_home_that_is_not_absolute_is_refused_naming_the_file_and_the_path` (:163) [the registry loads it]
- [x] `tddy-sandbox-recipes` `tests/lower_jail_sections_unit.rs`: `a_path_home_is_the_operators_directory_mounted_read_write` (:147) [TODO panic]
- `tddy-daemon-kernel`: "the type has no agent field" is compile-level — enforced by the deletion itself and by the fixture scan; no runtime test is written for it.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Answer hold | Integration with a hook report | the hold/timeout pairing is the hang risk |
| No-flicker env | Integration (host and jail) | the env previously reached the jail out of band |
| Fixture migration | Source scan | keeps the property |

## Decisions & Trade-offs

- **Breaking removal**, already refused at load by #801; this deletes what was left behind. Changelog entry required.
Settled at red (2026-11-05, the orchestrator's decisions under "finish without stopping"):

- ✅ **Settled — `claude_home_dir` is kept as a feature:** a path form of the spec's `home:` — `home: { kind: path, path: <dir> }` (`HomeSection::Path`), an absolute directory mounted read-write; a relative path is refused at load. Today's daemon.yaml `claude_cli.claude_home_dir` maps to a `<tddyhome>/tools/claude.yaml` override using it, and the start-up refusal names that replacement (`claude_home_dir → home: { kind: path, path: <dir> }`). This changes #801's refusal text for that one field, by decision.
- ✅ **Settled — hook urls:** per-agent hook urls collapse into `listen.advertise_url` (`daemon_hook_urls::local_daemon_hook_url`). No per-agent url config.
- ✅ **Settled — `coding_agents` is non-optional everywhere,** `LaunchSessions` and `DaemonCodingAgentLauncher` included. The agentless hand-built ACP launch path is deleted (§ Removed); `acp-hand-built-launch-path-for-agentless-launchers` is claimed ✅ RESOLVED HERE.
- ✅ **Settled — `answer_wait_secs` is per agent:** read from `coding_agent.answer_wait_secs`; the server-side hold (`svc_activity_ports.rs:103`) uses the session's recorded value, which is the one its hooks file was written with. Both pinned (acceptance).
- ✅ **Settled — fullscreen:** `CLAUDE_CODE_NO_FLICKER` is declared in claude.yaml `run.env` (published by the contract) and the out-of-band additions on the host, jail, resume and relaunch paths are removed, so a `<tddyhome>/tools/claude.yaml` override can turn it off — fixing n12's accepted regression. Pinned on the host and in the jail.
- ✅ **Settled — no "as-is" binary:** the binary resolvers' last step becomes a refusal naming the agent and the places searched (`tool claude is unavailable: searched TDDY_CLAUDE_BINARY (unset), PATH (<home>/.local/bin:…)`), through `host_coding_agents::daemon_tool_host`. No fallback.
- ✅ **Settled — Telegram delete** removes an agent session's worktree again, honouring `KeepWorktreeOnDelete`; `telegram-delete-leaves-agent-worktrees` is claimed ✅ RESOLVED HERE.
- ✅ **Settled — fixtures:** the ~40 fixtures migrate off `daemon_config_from_fixture_file` (to a `<tddyhome>/tools/<id>.yaml` override or `test_util::coding_agents_with_binaries`), then it is deleted. A changelog entry covers the breaking config change.
- ✅ **Settled at red — the answer-wait record is a file, not a `SessionMetadata` field:** `.session.yaml` is `deny_unknown_fields`, so a new key would make a downgraded daemon drop every session this one started, and the struct has ~143 literals across the workspace.
- Complexity 5: consider greening in this order — readers move up, answer wait, `run.env`, fixtures, then the deletion commit.

## TODO

- [x] Record initial discovery (`2026-11-04-agent-tools-agent-config-removal-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived: the developer asked the stack to finish without stopping; orchestrator decisions folded in, § Decisions)
- [x] TDD Red — write failing unit/integration tests
- [ ] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run the touched packages' tests (`./test -p {package}` per package) — verify 100% pass
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p {package} -- -D warnings`, `cargo fmt`)
- [ ] USER REVIEW — scope developed, every gap approved: ready to wrap

## Refactoring Needed
### From /red (TDD red phase)
### From /validate-changes
### From /validate-tests
### From /validate-prod-ready
### From /analyze-clean-code

## Validation Results
| Validation | Last Run | Status |
|------------|----------|--------|
| /validate-changes | Not yet run | Pending |
| /validate-tests | Not yet run | Pending |
| /validate-prod-ready | Not yet run | Pending |
| /analyze-clean-code | Not yet run | Pending |
