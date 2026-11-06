# 2026-11-06 — Claude and Cursor launch like every CLI agent; their dispatch is deleted

**Type:** Refactor (planned deletions), with one fix of this node's own regression

`#agent-tools` 17/19 ([#823](https://github.com/uppin/tddy-coder/pull/823)), branch
`feature/agent-tools/cli-dispatch-collapse`. Based on `feature/agent-tools/spec-backend`
([#822](https://github.com/uppin/tddy-coder/pull/822)); its real dependency is #820 (n14). Blocks
`feature/agent-tools/telegram-spec-launch` (n18).

Packages: `tddy-agent-launch`, `tddy-session-lifecycle` (suites), `tddy-worktree-service`,
`tddy-cli-sessions`, and for the session pointer `tddy-agent-backend`, `tddy-tools` and
`tddy-tools-cli` (tests). `tddy-daemon-sandbox` changes one test line.

## What changed

Every CLI agent starts, resumes and re-attaches through the one generic spec launch #820 finished:
`opencode_cli_spawn::start_cli_harness_session` / `resume_cli_harness_session` on the host and in
tmux, `cli_harness_tmux::reattach_recorded_harness` after a daemon restart, and
`cli_harness_jail` / `svc_start_sandboxed_cli_harness_session` in a jail. Claude and Cursor are
specs (`claude.yaml`, `cursor.yaml`), so no agent id is compared in `tddy-agent-launch`.

- **No coding agents → `FAILED_PRECONDITION`** on start, resume and a child spawn, with nothing
  spawned. Re-attach on such a host re-attaches nothing and logs it. A record with no session type
  is skipped with a warning; it is never re-attached as `claude-cli`.
- **Branch intent from the spec.** `harness_from_spec::cli_branch_intent` is
  `BranchIntentPolicy::cli_agent(session_type, spec.has(BranchNameRequired))`. For the built-ins it
  behaves exactly as `claude_cli()` / `cursor_cli()` did.
- **A child runs its parent's agent.** Conversation and pr-stack children run the parent session's
  agent (`child_session_type`, `cli_branch_starts::start_spawned_cli_child`) instead of a hard-coded
  claude. A parent with no CLI session type refuses the child.
- **Re-attach reads `HostCodingAgents`.** It no longer reads `<tddy_data_dir>/tools`
  (`recorded_cli_session_type`, `recorded_resume_token_adapter(agents, …)`,
  `require_sandbox_placement`). `reattach_recorded_main(manager, agents, tddy_data_dir, os_user,
  session_id)` keeps `tddy_data_dir`, which is where the record lives.
- **A source scan pins it.**
  `tddy-session-lifecycle/tests/no_agent_name_gates.rs::no_agent_id_literal_is_compared_or_dispatched_on_in_agent_launch`
  finds no `"claude"`, `"cursor"`, `"claude-cli"` or `"cursor-cli"` literal compared or dispatched
  on in `tddy-agent-launch`.
- **A split session resumes through the generic resume too.** `resume_cli_harness` re-derives the
  split wiring from the persisted pairing first (`SplitSessions::resume_split_wiring`: a fresh join
  token, the roster's withdrawals, the `TDDY_REMOTE_*` transport). A codebase host that cannot be
  read refuses the resume. The agent resumes in its context directory, with no commit identity of
  its own, and gets the pre-rendered split flags ahead of its role flags, as its split spawn does.
  The deleted `resume_claude_cli_session` used to do this, and wrap found the gap through
  `tddy-daemon/tests/split_session_resume_acceptance.rs`: 0/8 before, 8/8 after (three needed a
  fresh LiveKit).
- **`start_request_checks::runs_in_tmux`.** A session type ending in `-cli` may run in tmux; this
  replaces the `TMUX_RUN_SESSION_TYPES` list.

### The session pointer for a CLI agent's shell (fix of this node's own regression)

Found by green: a managed claude or cursor agent running `tddy-tools submit|transition|ask` from its
own shell no longer found its session. The bespoke launch had put `TDDY_SOCKET` on the agent
process, and the generic path carries it only in the tool-server registration. The generic path also
still gave the agent process the operator-notify `TDDY_*` variables, and three suites filtered those
names out of their "the agent sees no `TDDY_*`" assertions. Both are fixed with n15's pointer:

- `tddy-agent-launch::cli_session_pointer` writes `.tddy/session.json` (n15's `SessionPointer`) into
  the session's worktree at every start and resume. It carries the managed workflow's socket, session
  directory and repo, and the operator-notify session.
  - Written owner-only (0600) and added to the repo's `info/exclude`. A worktree in no git checkout
    gets no exclude, since nothing can commit the file there.
  - A stale pointer is replaced.
  - Removed when the agent exits: always for a PTY child; in tmux only when the launch already has
    an exit hook, because a hook keeps the pane. Also removed by `DeleteSession`.
  - An ssh checkout gets it through the bounded slot-file write. Exclude and removal are not done
    there ([backlog](../todo/2026-11-06-ssh-session-pointer-exclude-and-removal.md)).
  - A jail writes none: its agent's shell commands run on the host through the Shell relay, whose
    environment carries the session.
- `SessionPointer.socket` becomes `Option<PathBuf>` and the pointer gains a `notify` map. In
  `tddy-tools`, `ResolvedSession` gains both. `session_source::session_environment`, which is what
  `adopt_session` sets, takes only the five `notify_env` names from `notify`, so a planted pointer
  cannot set `PATH`.
- No `TDDY_*` variable reaches a CLI agent's process now, the notify ones included. The notify
  exemption is gone from `cli_spec_features_acceptance` (ssh), and the `claude_cli_from_spec` golden
  `host_start_env.txt` pins those names as absent.

## Removed

The planned deletions (the changeset's § Removed), all done:

- `CliLauncher`, `launcher_of`, `recorded_cli_launcher`
- `CLAUDE_AGENT_ID` (`harness_from_spec.rs`, `spec_launch.rs`) and `CURSOR_AGENT_ID`
  (`harness_from_spec.rs`, `cursor_from_spec.rs`)
- `spawn_hand_built`, `start_claude_in_pty`, `install_claude_cli_hooks`, and the hand-built resume arm
- `spawn_cursor_cli_process`, `prepare_cursor_cli_launch`
- `start_claude_in_tmux`, both `relaunch_claude_cli`s, `reattach_recorded_claude`, and the
  `unwrap_or("claude-cli")` default
- `launch_claude_cli`, `start_claude_cli_from_request`, `spawn_claude_cli_session_inner`
- `spawn_cursor_cli_session_inner`, `spawn_cursor_cli_session_reporting`, `spawn_cursor_cli_from_request`
- `start_sandboxed_claude_cli_from_request`, `start_sandboxed_cursor_cli_from_request`
- `resume_claude_cli_session`, `resume_cursor_cli`
- `launch_jailed_claude`, `launch_jailed_cursor`
- `BranchIntentPolicy::{claude_cli, cursor_cli, cli_harness}`
- the `TODO(agent-registry)` and `TODO(agent-tools-cascade)` markers in `harness_from_spec.rs`

**Files emptied by those deletions, and deleted:** `claude_jail.rs` (`ClaudeJail`; `closure_defs`
moved to `cli_harness_jail.rs`), `resume.rs` (`resume_cursor_cli_session`, `relaunch_cursor`;
`chat_to_resume` became `cli_agent_steps::resumed_or_adopted_conversation`),
`svc_resume_sandboxed_claude_cli_session.rs`, and `svc_start_sandboxed_cursor_cli_session.rs`.
Shrunk to what remains: `claude_cli_spawn.rs`, `cursor_cli_spawn.rs` (Telegram's two helpers),
`claude_cli_spawn_steps.rs`, `svc_resume_claude_cli_session.rs` (`resumed_recipe`), and
`svc_start_sandboxed_claude_cli_session.rs` (now only the generic `JailSession`, `JailBranch` and
`JailDirs` types; renaming it is the sweep's).

**Deleted beyond the planned names.** Each item was code that only the deleted dispatch called, or
a test that pinned deleted code:

| Item | Where (owning node) | Why it went |
|---|---|---|
| `RouteAgent::claude_on`, `CLAUDE_ROUTE_AGENT` | `session_provider_route.rs` (n13) | callerless once the bespoke claude routes went; the tests use a local `on_the_claude_agent` helper |
| `TMUX_RUN_SESSION_TYPES` | `start_request_checks.rs` (#tmux-agent) | the plan listed it as modified; it is replaced by `runs_in_tmux` (the `-cli` suffix), so no agent list remains |
| `refuse_a_managed_codebase_in_a_jail` | `cli_agent_steps.rs` (n14) | the generic jail now runs a managed workflow, as the bespoke jailed claude did (`docs/dev/todo/2026-11-05-generic-jail-managed-workflow.md`, resolved and deleted here) |
| tests `the_resolved_accounts_pairs_are_in_the_agents_environment`, `a_start_that_resolved_nothing_adds_no_git_variables`, helper `the_launch_environment_with` | `claude_cli_spawn_steps_tests.rs` | they pinned `managed_claude_cli_launch`'s environment passthrough, which was deleted. The generic start applies `session_identity_in` (`opencode_cli_spawn.rs`), and `stack_child_spawn_tests::a_conversation_the_agent_spawned_commits_as_the_account_its_orchestrators_owner_holds` asserts the commit identity end to end |
| tests `a_project_home_spawn_is_pre_approved_exactly_the_project_home_tool_set`, `an_ordinary_spawn_adds_no_allowed_tools`, helper `allowed_tools_in` | `claude_cli_spawn_steps_tests.rs` | they pinned the deleted `role_allowed_tools_args` (hand-built role flags). The spec-rendered tool lists are pinned by `project_home_launch_tests` and `cli_spec_features_acceptance` (project-home main) |
| `role_allowed_tools_args`; `record_role_before_spawn` (now `record_role_of_session_type_before_spawn`) | `claude_cli_spawn_steps.rs` | hand-built role flags; the role record takes the session type |
| `ClaudeCliWorktreeCut`, `cut_claude_cli_worktree`, `claude_cli_checkout`, `spawned_claude_cli_branch`, `ManagedClaudeCliLaunch`, `managed_claude_cli_launch`, `ClaudeCliProcess`, `SpawnedClaudeCli`, `spawn_claude_cli_process`, `spawn_from_spec`, `claude_cli_livekit_room` | `claude_cli_spawn_steps.rs` | the steps of the deleted `spawn_claude_cli_session_inner` |
| `ClaudeCliTmuxLaunch`, `start_agent_pane`, `resume_claude_in_tmux`, `reattach_claude_in_tmux` | `claude_cli_tmux.rs` | helpers of the deleted claude tmux paths |
| `HarnessTmuxLaunch::cursor` | `cli_harness_tmux.rs` (#tmux-agent) | callerless |
| `spawn_cursor_from_spec`, `resolve_cursor_agent`, `LaunchedCursor`, `no_coding_agents` | `cursor_cli_spawn.rs` (n7/n14) | callerless; `no_coding_agents_for` (`harness_from_spec`) refuses for every agent |
| `configured_cursor_binary` | `cursor_from_spec.rs` (n7) | callerless |
| `configured_binary`, `ClaudeCliLaunch` | `spec_launch.rs` (n2) | callerless |
| `start_claude_cli_session` | `svc_start_claude_cli_session.rs` | the body behind `start_claude_cli_from_request` |
| `start_sandboxed_claude_cli_session`, `jail_session_env` | `svc_start_sandboxed_claude_cli_session.rs` | the bespoke jailed start |
| `relaunch_sandboxed_runner` | `svc_relaunch_sandboxed_runner.rs` | the bespoke jailed relaunch; the generic one is `relaunch_sandboxed_cli_harness` |
| `resumed_notify`, `resumed_agent_env` | `svc_resume_claude_cli_session.rs` | helpers of the deleted resume |
| `sandboxed_claude_route` | `cli_branch_starts.rs` | helper of the deleted sandboxed start |
| `RoleLaunch`, `RoleWiring::hand_built`, `after_split`, `ResumedFlags`, `hand_built_args` ×2, `spec_launch`, `hand_built_role_args` | `project_home_launch.rs` (#project-home) | hand-built role rendering; `RoleLaunch` survives as a test-local struct |
| `cursor_cli_session_acceptance` helpers `ADaemonClaimingNoClones`, `ARecordingStackParentOwner`, `start_cursor_cli_session_under`, `branch_of`, `links_recorded`, `chain_base_ref`, `link_spawned_branch`, `claim_for_seed` | `tests/cursor_cli_session_acceptance.rs` | they called `spawn_cursor_cli_session_inner`. The suite now starts through `StartSession`, and the stack link is read from the orchestrator's changeset on disk |

Still alive, each with `#[allow(dead_code)]` and a `TODO(cli-dispatch-collapse)`, for the sweep (n20)
to delete once approved: `warm_up_jail_agents`, `jail_semantic_index_env`,
`project_default_branch_ref`, `create_jail_project_worktree`, `link_jail_branch_to_stack_node`,
`create_routed_session_dir`, `chat::{cursor_notify_env, resumed_cursor_notify_env}`
([backlog](../todo/2026-11-05-callerless-bespoke-launch-helpers.md)). Kept for Telegram (n18):
`install_cursor_hooks_in_worktree`, `mint_cursor_chat_id`, `CliSessionManager::{start,
start_cursor}` and the argv builders they use
([backlog](../todo/2026-11-03-cursor-hand-built-launch-paths.md), narrowed to them).

## Earlier nodes' code changed here

A done layer is frozen, so these changes belong to this node. All were found by green or wrap:

1. **`harness_from_spec.rs` (n12/n13/n14).** `CliSessionAgent` gains `renderer_env`, and the new
   `renderer_env()` applies `config.claude_cli`'s renderer variable to the agent whose binary is
   resolved through `TDDY_CLAUDE_BINARY` (`TODO(agent-config-removal)`; n19 moves it into
   `run.env`, per `2026-11-05-cli-spec-features-leftovers.md`). `recorded_resume_token_adapter`
   and `require_sandbox_placement` read `HostCodingAgents`.
2. **`cli_agent_steps.rs` (n14).**
   - Added: `resumed_or_adopted_conversation` (a resume mints the chat a session never recorded),
     `carry_on_the_tool_server` (a session with no role toolset still registers `tddy-tools --mcp`
     carrying its `TDDY_*` pairs), and `hooks_report_over_the_agent_tool_socket`.
   - Deleted: `refuse_a_managed_codebase_in_a_jail`.
   - `SessionWiring` and `ResumedWiring` gain `session_pointer`. The notify pairs go there, not
     to the process.
3. **`opencode_cli_spawn.rs` (n13/n14).**
   - `CliHarnessStart.renderer_env`.
   - The session pointer at start and resume.
   - The resume adopts the conversation and records `cursor_chat_id`.
   - `settle_unwatched_agent_exit` takes the host's agents.
4. **`cli_harness_jail.rs`, `svc_start_sandboxed_cli_harness_session.rs` (n13/n14).** The generic
   jail gains claude's and cursor's needs:
   - the managed workflow;
   - the `tddy-tools` MCP registration and its jail home;
   - tool allow/deny lists and the permission prompt (`JailToolset`);
   - the managed prompt as a flag or as a rules file;
   - the role;
   - the passthrough args;
   - hooks written into the worktree before the context copy (`write_jail_hooks`).

   Sanctioned by `2026-11-05-generic-jail-managed-workflow.md`, which named n17.
5. **`svc_resume_sandboxed_cli_harness_session.rs` (n13/n14).** A jailed resume re-wires the managed
   recipe, adopts the conversation and carries the renderer env.
6. **`session_provider_route.rs` and its tests (n12/n13).** `claude_on` and `CLAUDE_ROUTE_AGENT` are
   deleted.
7. **`start_request_checks.rs` (n14).** `TMUX_RUN_SESSION_TYPES` is replaced by `runs_in_tmux`.
8. **n15's session pointer** (`tddy-agent-backend/src/session_pointer.rs`, `backend/spec_headless.rs`,
   `tddy-tools/src/session_source.rs` and their tests).
   - `socket: Option`, plus `notify`.
   - `adopt_session` delegates to `session_environment`, which filters `notify`.
   - The headless backend writes `socket: Some(..)` and an empty `notify`, so its behaviour is
     unchanged.
9. **`cursor_from_spec.rs`, `spec_launch.rs` (n12/n14) and `cli_branch_starts.rs` (n13/n14).**
   - The agent-id constants and binary helpers are deleted.
   - The claude and cursor request starts give way to `SpawnedChild` / `start_spawned_cli_child`.
   - `test_util::builtin_coding_agents_for(config)` (n12) resolves `tddy-tools` the way a daemon
     does.

## Tests

New:

- `tddy-agent-launch/src/harness_from_spec/cli_agent_dispatch_unit_tests.rs`
- `tddy-agent-launch/src/cli_session_pointer/cli_session_pointer_unit_tests.rs` (6)
- `tddy-session-lifecycle/src/connection_service/stack_child_spawn_agent_tests.rs`
- `no_agent_name_gates.rs::no_agent_id_literal_is_compared_or_dispatched_on_in_agent_launch`
- `claude_cli_tmux_reattach_acceptance` (re-attach from the installed agents; a record with no
  session type is not re-attached)
- `claude_cli_tmux_session_acceptance` (the spec's argv in tmux)
- `yaml_only_agent_acceptance::removing_branch_name_required_…`
- `cli_spec_features_acceptance::a_managed_claude_copys_shell_run_tddy_tools_submit_reaches_its_session_with_no_tddy_variable_in_the_agent`
  — red without the pointer, which fails with "no session to report to"
- `tddy-tools-cli/tests/notify_cli_acceptance.rs::notify_reads_its_session_from_the_pointer_when_its_environment_names_none`
- `tddy-tools/tests/session_source_unit.rs::a_pointers_notify_map_sets_only_the_names_tddy_tools_notify_reads`

Re-pointed: the claude and cursor suites in `tddy-session-lifecycle`. The `stack_child_spawn_tests`
stub now records its command line by rename; a torn read failed one test per `--lib` run under load.

## Code issues (`tddy-agent-launch` unless named)

Closed and deleted. Each final number is base `d325fba73` → HEAD; function lines run from the
`fn` line to the closing brace, and file sizes are production lines:

| Record | Final measurement |
|---|---|
| `complexity-claude-cli-spawn-spawn-claude-cli-session-inner` | 329 → deleted |
| `complexity-claude-jail-launch-jailed-claude` | 95 → deleted with its file |
| `complexity-cursor-cli-spawn-spawn-cursor-cli-session-inner` | 78 → deleted |
| `complexity-svc-resume-claude-cli-session-resume-claude-cli-session` | 158 → deleted |
| `complexity-svc-resume-sandboxed-claude-cli-session-…` | 79 → deleted with its file |
| `complexity-svc-start-sandboxed-claude-cli-session-…` | 281 → deleted |
| `crap-svc-start-sandboxed-cursor-cli-session` | 360 → deleted with its file |
| `heavy-params-svc-relaunch-sandboxed-runner-…` | 14 params, 129 lines → deleted |
| `oversized-file-svc-start-sandboxed-cursor-cli-session` | 552 → file deleted |
| `oversized-file-cursor-cli-spawn` | 684 → 9 |
| `oversized-file-claude-cli-spawn-steps` | 558 → 93 |
| `oversized-file-spec-launch` | 525 → 408 |
| `oversized-file-svc-start-session-core` | 502 → 415 |

Narrowed (better, still open):

- `start_here`: 348 → 265
- `resume_session_at_session_coordinate`: 197 → 172
- `start_cli_harness_session`: 303 → 301
- `tddy-session-lifecycle` `oversized-file-connection-service`: −1 line

Regressed (kept with the new number; restructuring is the sweep's, n20, by the developer's
2026-11-04 decision):

- `resume_cli_harness_session`: 97 → 121
- `opencode_cli_spawn.rs`: 632 → 685 production lines
- `cli_agent_steps.rs`: 571 → 648 production lines
- `wire_session_agent`: 80 → 87
- `resumed_session_wiring`: 72 → 79
- `start_sandboxed_cli_harness_session`: 239 → 278
- `resume_sandboxed_cli_harness`: 74 → 87
- `launch_jailed`: 150 → 151
- `start_cli_harness_from_request`: 89 → 90
- `tddy-agent-backend` `invoke`: 103 → 104
- `tddy-agent-backend` `spec_headless.rs`: 645 → 646 production lines

New records: `complexity-svc-resume-sandboxed-cli-harness-session-relaunch-sandboxed-cli-harness`
(87 → 127), `complexity-cli-harness-jail-launch-jailed-cli-agent` (41 → 82) and
`complexity-claude-cli-tmux-reattach-recorded-main` (70 → 76).

Unchanged: `tddy-worktree-service` `resolve_branch_workflow` (86).

## Backlog

- **Deleted (resolved here):** `2026-11-04-claude-and-cursor-launch-by-agent-id`,
  `2026-11-02-claude-cli-hand-built-launch-removal`, `2026-11-05-generic-jail-managed-workflow`.
- **Narrowed:** `2026-11-03-cursor-hand-built-launch-paths` (to Telegram's four helpers, n18) and
  `2026-11-05-cli-spec-features-leftovers` (the shell's `tddy-tools` reaches its session through
  the pointer).
- **Added:**
  - `2026-11-05-callerless-bespoke-launch-helpers` (n20)
  - `2026-11-06-ssh-session-pointer-exclude-and-removal`
  - `2026-11-06-jail-tool-ipc-socket-collides-across-concurrent-sessions`: a pre-existing
    `tddy-sandbox` defect behind ten flaky jailed tests in this run, each passing with
    `--test-threads=1`
