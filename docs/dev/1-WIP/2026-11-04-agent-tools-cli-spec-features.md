# Changeset: The generic CLI spec launch serves everything claude and cursor need

**Created:** 2026-11-04
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 5 — re-scored at the contract (2026-11-05), unchanged: four crates (`tddy-coding-agent-specs` + a slot module, `tddy-agent-backend` a new adapter trait, `tddy-agent-launch` a new step module and four touched files, `tddy-session-lifecycle` fixture + 12 acceptance tests); spawned-process (`create-chat`), tmux and ssh seams each with a deadline; the conversation slot now has a settled shape, but the green work also has to lift two gates the plan did not list (`project_home_start_spec::refuse_a_type_main_cannot_run`'s `claude-cli` literal, the generic start's ignored `ssh_config_host`), and it grows three files with open complexity records
**Stack:** `#agent-tools 14/19` — branch `feature/agent-tools/cli-spec-features`, base `feature/agent-tools/route-support` (#819)
**PR:** [#820](https://github.com/uppin/tddy-coder/pull/820) (draft)
**PRD:** none of its own. Serves the registry PRD on `feature/agent-tools/agent-registry` (`docs/ft/daemon/1-WIP/PRD-2026-10-31-coding-agent-registry.md`): "adding an agent is a YAML file" for an agent with claude's or cursor's needs

## Initial Discovery

[2026-11-04-agent-tools-cli-spec-features-initial-discovery.md](./2026-11-04-agent-tools-cli-spec-features-initial-discovery.md)

State A below is distilled from that file; its split table says which sections are this node's.

## Prerequisites

| Record | Verdict |
|---|---|
| [2026-11-04-claude-and-cursor-launch-by-agent-id.md](../todo/2026-11-04-claude-and-cursor-launch-by-agent-id.md) | ⚠ During. This node makes the generic path able to launch claude and cursor; the dispatch that still picks their bespoke paths is deleted by `feature/agent-tools/cli-dispatch-collapse` (n17), which claims the record |
| [2026-11-02-claude-cli-hand-built-launch-removal.md](../todo/2026-11-02-claude-cli-hand-built-launch-removal.md) | ⚠ During. Claimed by n17 |
| [2026-11-03-cursor-hand-built-launch-paths.md](../todo/2026-11-03-cursor-hand-built-launch-paths.md) | ⚠ During. Claimed by n17 |
| [2026-11-03-harness-clis-legacy-cli-harness-removal.md](../todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md) | — Reference. `refuse_an_unsupported_cli_session_start` (which this node changes) superseded the legacy refusal listed there; the legacy one is not touched |
| `packages/tddy-agent-launch/docs/code-issues/complexity-opencode-cli-spawn-start-cli-harness-session.md`, `complexity-opencode-cli-spawn-resume-cli-harness-session.md`, `complexity-cli-branch-starts-start-cli-harness-from-request.md` | ⚠ During. These are the generic functions that grow. New behaviour goes in new step functions beside them, not inline; restructuring is the sweep node's. Re-measure at wrap |
| `packages/tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md` | ⚠ During. Untouched here except the managed refusal call; n17 shrinks it |

## Affected Packages

- [ ] [`tddy-coding-agent-specs`](../../../packages/tddy-coding-agent-specs/README.md): `Capability::ManagedWorkflow` (claude, cursor YAMLs); a conversation-id slot; the template parameters `session_id`, `permission_mode`, `dangerously_skip_permissions`, `chat_id`
- [ ] [`tddy-agent-backend`](../../../packages/tddy-agent-backend/README.md): `LaunchAdapters::conversation_id("cursor-create-chat")`, with a deadline
- [ ] [`tddy-agent-launch`](../../../packages/tddy-agent-launch/README.md): the generic start/resume (`start_cli_harness_session`, `resume_cli_harness_session`, `harness_launch.rs`, `cli_harness_jail.rs`) gains the slots and steps below; `write_slot_file_over_ssh` gets a deadline
- [ ] [`tddy-session-lifecycle`](../../../packages/tddy-session-lifecycle/README.md): acceptance suites

## Responsibility

The generic spec launch path (`cli_branch_starts::start_cli_harness` → `opencode_cli_spawn::start_cli_harness_session`, its resume, `harness_launch.rs` tmux, `cli_harness_jail::launch_jailed_cli_agent`) gains, each keyed on a spec slot or capability, never on an agent id:

- **Managed workflow.** `Capability::ManagedWorkflow`, declared by `claude.yaml` and `cursor.yaml`. `refuse_an_unsupported_cli_session_start` (`start_request_checks.rs:64`) refuses `managed_codebase` only for a spec without it; with it, the generic start resolves `placement::managed_recipe_for` and appends the recipe prompt as `managed_claude_cli_launch` does today.
- **Hooks.** When `spec.has(Hooks)`, `render_host_slots` renders the spec's `hooks` slot (today it passes `hooks: None`, `opencode_cli_spawn.rs:114-137`), with the hook timeout from `answer_wait_secs` as today.
- **MCP, tool-list and system-prompt slots** filled from the role toolset (`AgentToolset`), as `spec_role_launch` does for claude.
- **Project-home role** (`Capability::ProjectHomeMain`): role allowed tools and `record_role_before_spawn` move into a generic step.
- **Operator notify** (`Capability::OperatorNotify`): `notify_env::agent_notify_unless_role`.
- **SSH remote checkout**: slot files written on the ssh target (keeping `d2a096b14`'s fix) and the remote working directory, for any spec.
- **Participant metadata / pr-stack link** recorded for any CLI agent session.
- **Conversation-id slot**: the spec declares how a session's conversation is identified — claude: the daemon session id (`--session-id`, `--resume` on resume); cursor: a chat id minted by the `cursor-create-chat` launch adapter (wrapping `chat::mint_cursor_chat_id`); opencode/muse: the worktree (`--continue`); warp: its existing `resume.adapter` token.
- **tmux start and resume** render through `spec_launch::plan_host_pty` → `start_harness_in_tmux` / `resume_harness_in_tmux` for every agent, closing the two `TODO(agent-tools-cascade)` markers at `claude_cli_spawn_steps.rs:401` and `svc_resume_claude_cli_session.rs:138`.
- **Jailed start/resume**: `launch_jailed_cli_agent` takes claude's and cursor's specs (closure defs, jail hook files, persistent homes) so the generic jailed path launches them.
- **Deadlines**: `mint_cursor_chat_id` and `write_slot_file_over_ssh` each get a deadline and a refusal naming the agent (or ssh host) and the deadline.

After this node, a copy of `claude.yaml` or `cursor.yaml` dropped in under another id launches through the generic path with the same argv, files and environment the bespoke paths produce. The built-in `claude`/`cursor` ids still dispatch to the bespoke paths until n17.

## Boundaries

- **Does not** delete `CliLauncher`, `launcher_of`, `CLAUDE_AGENT_ID`/`CURSOR_AGENT_ID`, any bespoke spawn, hand-built arm or `BranchIntentPolicy` constructor: all of that is `feature/agent-tools/cli-dispatch-collapse` (n17).
- Does not touch Telegram's launcher (`feature/agent-tools/telegram-spec-launch`, n18).
- Does not touch `daemon_config_from_fixture_file` callers or the `config.claude_cli`/`cursor_cli` readers (`feature/agent-tools/agent-config-removal`, n19). New tests use `with_coding_agents` fixtures.
- No proto or web change. (Hiding the managed toggle for agents without `ManagedWorkflow` would be a web change; not here.)
- Restructuring and file splits are the sweep node's; code-issue records only.

**Stack-wide rules this node keeps:**
- The agent process sees no `TDDY_*` variable: the generic slots render only spec-declared env; a test asserts the stub's environment has no `TDDY_` key.
- No fallback without coding agents: the generic path refuses `FailedPrecondition` (it already does).
- No hangs: the two new deadlines; tmux pane start stays bounded (`in_tmux_blocking`).
- Fixtures install coding agents and the sandbox RPC bridge; Linux jail tests self-skip without user namespaces, macOS-only jail tests sit in a `jailed` module.
- Deletions not listed in **Removed** need the developer's approval.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n13` route-support (#819) | `RoutedAgent`, `routed_agent(&spec)`, agent-keyed `RouteRefusal`; `CliSessionType.route: RouteSupport` | the generic path's route already takes the spec; claude/cursor copies route through it | add route bodies, touch `Harness` (gone) |
| `n12` agent-registry (#801) | `Capability` (closed enum), `CodingAgentRegistry`, `HostCodingAgents`, `spec.has` | adds one variant, `ManagedWorkflow`; gates the new steps on capabilities | add other capabilities or change gate semantics |
| `n8` harness-clis (#797), `n7` cursor-cli (#796), `n2` claude-host (#791) | `launch_host_pty`, `plan_host_pty`, `launch_jailed_cli_agent`, the `claude`/`cursor` specs and their slot renderers | extends the generic launch with their slots | change the specs' existing argv |
| `n12` agent-registry (#801) | `slot_file_over_ssh::write_slot_file_over_ssh` bounded by `SLOT_FILE_OVER_SSH_DEADLINE` (60 s, `1818fb470`) | the generic ssh step reuses it; `SpecLaunchError::SlotFileOverSshTimedOut` becomes `DEADLINE_EXCEEDED` | add a second ssh deadline |

**Sequencing recorded at the contract (6f).** No n14 test asserts n13 behaviour, and none injects a
double for it: every n14 acceptance start runs **unrouted** (no provider account), so it never reaches
n13's `TODO(route-support)` bodies (`routed_agent`, `CodingAgentSpec::product`, `by_spec`). The one
exception is the fixture file itself: `agent_registry_support/mod.rs` and `yaml_only_agent_acceptance.rs`
carry n13's red routed tests too, so n13's green and n14's green touch the same two files (additions
only on both sides — a textual merge, not an edge). The refusal "naming the agent" names `spec.id`, not
`product()`, so it does not wait on n13's `product` body.

## Draft PR contract

Published by the contract commit (unimplemented bodies are `// TODO(cli-spec-features): implement`):

| Crate | Symbol | Body |
|---|---|---|
| `tddy-coding-agent-specs` | `Capability::ManagedWorkflow` (`managed_workflow`, in `ALL`) | done; **the YAML lines are green's** (`claude.yaml`, `cursor.yaml`), pinned by `claude_and_cursor_declare_managed_workflow_and_no_other_builtin_does` — green adds them to `agent_registry_unit::every_builtin_agent_declares_the_capabilities_its_old_gates_granted` too |
| | `slots::conversation::{ConversationSlot, DEFAULT_CONVERSATION_MINT_DEADLINE_SECS}` — `coding_agent.conversation: { kind: daemon_session_id \| minted \| worktree \| resume_token }`, `minted` with `adapter` and optional `mint_deadline_secs` (30 s default) | types done; `ConversationSlot::mint_deadline` TODO |
| | `CodingAgentSection.conversation`, `CodingAgentSpec::conversation()` (derived when absent: `resume_token` with a `resume` slot, else `worktree`) | TODO; the load refusals (unknown adapter, `resume_token` without `resume`) are a TODO in `CodingAgentExtension::validate` |
| `tddy-agent-backend` | `adapters::conversation_id::{ConversationIdAdapter, ConversationMint, ConversationMintError, CursorCreateChat, CURSOR_CREATE_CHAT}`; `LaunchAdapters::{conversation_id, conversation_id_names}` (registered in `builtin()`) | registry done; `CursorCreateChat::mint` TODO |
| `tddy-agent-launch` | `HarnessLaunch.{session_id, chat_id, permission_mode, dangerously_skip_permissions}` | fields done; `harness_template_params` binds them — TODO |
| | `refuse_an_unsupported_cli_session_start(spec, session_type, req)` — now reads the spec; `start_cli_harness` resolves the agent first (no side effect) to pass it | signature done; capability gate TODO |
| | `cli_agent_steps` (new, `#[allow(dead_code)]` until green wires it): `HostLaunchSlots`, `render_host_launch_slots`, `write_slot_files_on_target`, `LaunchConversation::{bound, recorded_chat_id}`, `ConversationStart`, `start_conversation`, `resumed_conversation`, `conversation_mint_refused`, `refuse_a_role_the_agent_cannot_hold` | TODO |
| `tddy-session-lifecycle` (tests) | `agent_registry_support`: `builtin_yaml_as`, `cursor_copy_minting_within`, `StubCreateChat`, `MINTED_CHAT_ID`, `ATddyVariableInTheDaemon`, `a_tmux_start`/`a_managed_start`/`an_ssh_start`, `ensure_project_home`, `stop`, `resume`, `worktree_of`, `AStubReport::{values_of, tddy_variables, tddy_tools_on_path}` | done |

Green also has to lift two gates the plan did not name: `project_home_start_spec::refuse_a_type_main_cannot_run` (`READ_ONLY_SESSION_TYPE = "claude-cli"`, a name gate) becomes `spec.has(ProjectHomeMain)`; and the generic start honours `ssh_config_host` (it cuts a local worktree today).

## Green wave

**Wave:** 2 of 5
**Greenable independently:** yes, once n13 is green — its tests drive dropped-in copies of the claude/cursor specs through the generic path and need only n13's route surface
**Concurrent with:** `feature/agent-tools/headless-driver` (n15)
**Blocks:** `feature/agent-tools/cli-dispatch-collapse` (n17: it can delete the bespoke paths only once the generic path serves claude and cursor)

Real dependency edges, as opposed to the branch line:

    n12 → n13      n13 → n14, n15, n16      n15 → n16      n14 → n17      n17 → n18      n16, n18 → n19

## State A

- `harness_from_spec.rs:123-179` dispatches `CliLauncher::{Claude, Cursor, Generic}` by spec id; only `Generic` takes the spec path.
- The generic start refuses `managed_codebase` for every CLI agent (`start_request_checks.rs:64-74`).
- The generic host launch renders no hooks, MCP, tool-list or system-prompt slot (`opencode_cli_spawn.rs:114-137`); has no project-home role, notify, ssh checkout, participant metadata or daemon-session-id conversation.
- Claude tmux start/resume are hand-built (`claude_cli_spawn_steps.rs:393-423` → `claude_cli_tmux.rs:61`; `svc_resume_claude_cli_session.rs:137-153` → `claude_cli_tmux.rs:354`); generic tmux uses `plan_host_pty` (`harness_launch.rs:110-129`).
- Claude and cursor jails: `claude_jail.rs:59 launch_jailed_claude`, `svc_start_sandboxed_cursor_cli_session.rs:430 launch_jailed_cursor`; generic: `cli_harness_jail.rs:53`.
- `chat::mint_cursor_chat_id` (`chat.rs:121-138`) calls `Command::output()` with no deadline; `spec_launch.rs:300 write_slot_file_over_ssh` blocks on `wait_with_output()` with no deadline.

## State B

- The generic CLI launch covers host PTY, tmux, jail, resume, managed workflow, hooks, role, notify, ssh and conversation id, every feature keyed on a spec slot or capability.
- A YAML-only agent declaring claude's capabilities runs a managed workflow with hooks; one declaring a minted conversation id resumes its chat.
- Neither the cursor chat mint nor an ssh slot write can hang a start.

## Delta

### New
- `packages/tddy-coding-agent-specs/src/slots/conversation.rs` (conversation-id slot)
- `packages/tddy-agent-backend/src/adapters/conversation_id.rs` (`cursor-create-chat`)
- Generic step modules in `packages/tddy-agent-launch/src/` beside `opencode_cli_spawn.rs` (hooks/role/notify/ssh/participant steps), names settled at red
- `packages/tddy-session-lifecycle/tests/cli_spec_features_acceptance.rs`
### Modified
- `tddy-coding-agent-specs`: `capability.rs`, `spec.rs`, `specs/{claude,cursor}.yaml` (+ `managed_workflow`, conversation slot), the other 7 YAMLs only if the slot needs an explicit value
- `tddy-agent-launch`: `start_request_checks.rs`, `harness_from_spec.rs` (`harness_template_params`), `opencode_cli_spawn.rs`, `opencode_cli_spawn/harness_launch.rs`, `cli_harness_jail.rs`, `cli_branch_starts.rs`, `spec_launch.rs` (`write_slot_file_over_ssh` deadline), `chat.rs` (adapter + deadline)
- `tddy-session-lifecycle/tests/yaml_only_agent_acceptance.rs` (new cases)
- Docs at wrap: `docs/ft/daemon/claude-cli-session.md`, `cursor-cli-session.md`, `opencode-cli-session.md`, `session-agent-roster.md`, `docs/ft/coder/managed-codebase-workflow.md`, `packages/tddy-agent-launch/docs/module-layout.md`, `packages/tddy-coding-agent-specs/docs/coding-agent-specs.md` (conversation slot, `managed_workflow`)
### Removed
- `claude_template_params` (`spec_launch.rs:452`) and `cursor_template_params` (`cursor_from_spec.rs:56`), superseded by the widened `harness_template_params`
- The two `TODO(agent-tools-cascade)` markers on the tmux start and resume
- Nothing else: the bespoke paths stay reachable until n17

## Testing Strategy

### Acceptance tests

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/yaml_only_agent_acceptance.rs`** (fixture `tests/agent_registry_support/mod.rs` `a_daemon_with_tools`, stub reports argv, env and files)

- [ ] `a_dropped_in_agent_declaring_hooks_and_managed_workflow_starts_over_a_managed_codebase_with_its_hooks_file_written_and_the_recipe_prompt_appended`
- [ ] `a_dropped_in_agent_without_managed_workflow_is_refused_a_managed_codebase_naming_the_agent`
- [ ] `a_dropped_in_agent_whose_conversation_is_the_daemon_session_id_resumes_with_the_same_id`
- [ ] `a_dropped_in_agent_whose_conversation_is_minted_by_cursor_create_chat_resumes_its_minted_chat`
- [ ] `the_agent_process_of_a_dropped_in_agent_sees_no_tddy_variable`

**`tddy-session-lifecycle`: `packages/tddy-session-lifecycle/tests/cli_spec_features_acceptance.rs`** (a copy of the built-in `claude.yaml`/`cursor.yaml` under another id)

- [ ] `a_copy_of_claude_yaml_under_another_id_starts_in_tmux_with_the_argv_the_claude_spec_renders_and_resumes_the_same_way`
- [ ] `a_copy_of_claude_yaml_under_another_id_as_a_project_home_main_records_its_role_and_restricts_its_tools`
- [ ] `a_copy_of_claude_yaml_under_another_id_on_an_ssh_host_writes_its_slot_files_on_the_target_only`
- [ ] `a_copy_of_cursor_yaml_under_another_id_starts_with_its_hooks_and_a_minted_chat_id`
- [ ] `jailed::a_copy_of_claude_yaml_under_another_id_starts_in_a_jail_with_its_closure_tools` (macOS only)
- [ ] `a_cursor_chat_mint_that_never_returns_refuses_the_start_after_its_deadline_naming_the_agent`
- [ ] `an_ssh_slot_write_that_never_returns_refuses_the_start_after_its_deadline_naming_the_host`

### Unit / integration tests

- [ ] `tddy-coding-agent-specs`: `managed_workflow` parses; claude and cursor declare it, the others do not; the conversation slot parses each kind and refuses an unknown adapter at load naming file and word.
- [ ] `tddy-agent-launch`: `harness_template_params` binds `session_id`, `permission_mode` (default `auto`), `dangerously_skip_permissions`, `chat_id`; opencode/muse/warp argv unchanged (golden); `refuse_an_unsupported_cli_session_start` per capability.
- [ ] `tddy-agent-backend`: `cursor-create-chat` returns the chat id a stub prints; times out against a stub that sleeps.

### Red at the contract (2026-11-05)

Every test below fails for n14's own missing implementation; none waits on n13's green (all starts are unrouted).

| Test | Observed failure |
|---|---|
| `yaml_only_agent_acceptance`: `…hooks_and_managed_workflow_starts_over_a_managed_codebase…`, `the_agent_process_of_a_dropped_in_agent_sees_no_tddy_variable` | the start is refused `managed_codebase is not supported for acme-cli sessions` (no capability gate yet). The TDDY test turns into a stack-rule guard once green admits the start |
| `…without_managed_workflow_is_refused_a_managed_codebase_naming_the_agent` | the refusal does not name the agent or `managed_workflow` |
| `…conversation_is_the_daemon_session_id_resumes_with_the_same_id` | `argv placeholder {session_id} is unbound` |
| `…conversation_is_minted_by_cursor_create_chat_resumes_its_minted_chat` | no `--resume CHAT` on either launch, no `cursor_chat_id` recorded (nothing is minted). Stop and resume already work in the fixture |
| `cli_spec_features_acceptance`: tmux, ssh-target, jailed claude copy | `argv placeholder {session_id} is unbound` |
| `…as_a_project_home_main…` | `only claude-cli can run main` (`refuse_a_type_main_cannot_run`'s name gate) |
| `…cursor_yaml…starts_with_its_hooks_and_a_minted_chat_id` | no `--resume`, no chat recorded, no `.cursor/hooks.json` |
| `a_cursor_chat_mint_that_never_returns…` | the start succeeds: nothing is minted |
| `an_ssh_slot_write_that_never_returns…` | the start fails `Internal` (unbound `session_id`), not `DEADLINE_EXCEEDED`. At green it waits the real 60 s ssh deadline, on purpose |
| specs `cli_spec_features_unit` (6 of 8) | `todo!` in `conversation()` / `mint_deadline()`; claude and cursor do not declare `managed_workflow` yet; the two load refusals load instead. `managed_workflow_is_written_as_its_snake_case_word` and `an_unknown_conversation_kind_is_refused_at_load` already pass at the contract: type-level guards |
| backend `conversation_id_adapter_unit` (3 of 4) | `todo!` in `CursorCreateChat::mint`; the registry test passes |
| agent-launch `cli_agent_steps_tests` (11) | `todo!` in each step |
| agent-launch `harness_from_spec` (4 new) | the new parameters are not bound (`{session_id}` unbound for claude, no `--resume` for cursor). `opencode_muse_and_warp_render_the_same_argv…` passes: a golden guard |
| agent-launch `start_request_checks::managed_workflow_tests` (2 of 3) | the gate refuses regardless of the spec; `an_unmanaged_start_of_any_agent_is_admitted` passes: a guard |

**Pre-existing / parent red in the scoped baseline** (`origin/feature/agent-tools/route-support` 38937a5f2; not n14's): n13's contract red is `route_by_spec_unit` (20), `routed_agent_unit` (5), `route_body_drift_acceptance` (4), `harness_specs_acceptance::opencode_yaml_requires_a_route…` (1), `spec::tests` product (2), `yaml_only_agent_acceptance` routed/profile (3) and `harness_clis_from_spec_acceptance` routed/profile/jailed (4). Locally, a run without the `./test` bins built (`tddy-tools-cli`, `tddy-acp-stub`, `tddy-sandbox-runner`) also fails `acp_driver_*`, `clone_askpass_acceptance` and every session-lifecycle start with "tool tddy-tools is unavailable". That is environmental: all of them pass once the bins are built.

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Feature parity | Integration, stub binary, spec copy under another id | proves the generic path without depending on the dispatch n17 deletes |
| Deadlines | Integration with a never-returning stub | the stack rule is that nothing hangs |
| Template parameters | Unit golden | byte-identity with the bespoke argv |

## Decisions & Trade-offs

- **Spec copies under another id** are the test vehicle: they reach the generic path today without a test-only branch.
- ✅ **Settled (orchestrator, 2026-11-05): the jailed claude/cursor start is in n14's scope** — n17 cannot delete the dispatch while the jailed starts have no generic equivalent.
- ✅ **Settled: the conversation-id slot** is `coding_agent.conversation { kind, adapter }` (a serde-tagged `ConversationSlot`), not an extended `ResumeSlot`: claude `daemon_session_id` (`--session-id`, `--resume` on resume, rendered from the session id); cursor `minted` by the `cursor-create-chat` launch adapter, recorded on `.session.yaml` `cursor_chat_id` (field rename left to the sweep); opencode/muse `worktree`; warp `resume_token`. **The mint has a deadline**: `mint_deadline_secs`, default 30 s; past it the start is refused `DEADLINE_EXCEEDED` naming the agent id, the adapter and the deadline, and nothing is spawned.
- ✅ **Settled: ssh remote checkout is generic** for every CLI agent, not behind a capability. Slot files go to the target through n12's `write_slot_file_over_ssh`, reusing its 60 s `SLOT_FILE_OVER_SSH_DEADLINE`; a timeout is `DEADLINE_EXCEEDED` naming the host.
- ✅ **Settled: `tddy-session-split/src/svc_spawn_split_agent.rs:262` (`CliSessionManager::start_with_options`) is out of scope.**
- ✅ **Settled: `mint_cursor_chat_id` and `write_slot_file_over_ssh` get deadlines** — the first through the adapter above (green makes `chat::mint_cursor_chat_id` delegate to it), the second is n12's, reused.
- **Settled at the contract — resume binds the recorded model for an id conversation.** The bespoke claude resume passes `--model`, so a resume whose conversation is `daemon_session_id` or `minted` binds the recorded model; a `worktree`/`resume_token` resume binds none (opencode/muse/warp argv unchanged, pinned by a golden).
- **Settled at the contract — the `auto` permission default.** `harness_template_params` binds `permission_mode` only when the start chose one; `auto` is green's choice of either a params default or a spec `else` branch, pinned by `the_claude_spec_renders_through_the_generic_parameters_the_argv_the_bespoke_launch_built`. If it is a params default, the two existing exact-params tests in `harness_from_spec` get the binding too.
- **Settled at the contract — the managed refusal resolves the agent first.** `start_cli_harness` resolves the session's agent (side-effect free) before refusing, so the refusal reads the spec. A managed start of an agent whose binary does not resolve now reports the resolution failure instead.

## TODO

- [x] Record initial discovery (`2026-11-04-agent-tools-cli-spec-features-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (waived for this run by the developer's "finish without stopping"; orchestrator, 2026-11-05)
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
