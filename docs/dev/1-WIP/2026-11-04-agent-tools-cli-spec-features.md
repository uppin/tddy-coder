# Changeset: The generic CLI spec launch serves everything claude and cursor need

**Created:** 2026-11-04
**Status:** 🚧 In Progress
**Type:** Feature
**Complexity**: 5 — several row-4 signals: crates `tddy-agent-launch`, `tddy-coding-agent-specs` (+ 9 YAMLs for a new capability), `tddy-agent-backend` (a launch adapter), `tddy-session-lifecycle` suites; spawned-process, tmux and ssh I/O seams with new deadlines; a new spec slot whose shape is not settled (conversation id); inherited code issues on every file it grows
**Stack:** `#agent-tools 14/19` — branch `feature/agent-tools/cli-spec-features`, base `feature/agent-tools/route-support` (#819)
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

## Draft PR contract

The first push of this PR publishes: `Capability::ManagedWorkflow` and its YAML lines; the conversation-id slot types; the `cursor-create-chat` adapter signature with its deadline; the generic step functions' signatures (`HostLaunchSlots`, the widened `HarnessLaunch` template parameters); and the failing tests below. Unimplemented bodies are `// TODO(cli-spec-features): implement`.

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

### Test level decisions

| Aspect | Level | Rationale |
|---|---|---|
| Feature parity | Integration, stub binary, spec copy under another id | proves the generic path without depending on the dispatch n17 deletes |
| Deadlines | Integration with a never-returning stub | the stack rule is that nothing hangs |
| Template parameters | Unit golden | byte-identity with the bespoke argv |

## Decisions & Trade-offs

- **Spec copies under another id** are the test vehicle: they reach the generic path today without a test-only branch.
- **Jail is in scope** although the brief lists only host and tmux: n17 cannot delete the dispatch while the jailed claude/cursor starts have no generic equivalent. **Open — confirm with the developer**, or move the jail fold to n17.
- Open: the conversation-id slot shape (`coding_agent.conversation { kind, adapter }` versus extending `ResumeSlot`); whether ssh remote checkout is generic or gated by a new capability (proposed: generic).
- Open: `tddy-session-split/src/svc_spawn_split_agent.rs:262` still calls `CliSessionManager::start_with_options`; out of scope here.

## TODO

- [x] Record initial discovery (`2026-11-04-agent-tools-cli-spec-features-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create changeset (this document)
- [ ] Create failing acceptance tests
- [ ] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [ ] TDD Red — write failing unit/integration tests
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
