# Initial discovery: the generic CLI spec launch grows what claude and cursor need (#agent-tools 14/19)

**Created:** 2026-11-04
**Changeset:** [2026-11-04-agent-tools-cli-spec-features.md](./2026-11-04-agent-tools-cli-spec-features.md)

Read-only discovery at base `0dbba9195` (`feature/agent-tools/agent-registry`, #801). Exploration 1 covered the whole CLI-launch collapse and is copied verbatim. It was split into two nodes:

| Discovery section | This node (n14, `feature/agent-tools/cli-spec-features`) | `feature/agent-tools/cli-dispatch-collapse` (n17) |
|---|---|---|
| §2 seams 1–6 (template params, conversation id, host slots, managed workflow, tmux, jail) | ✅ | — |
| §2 seam 7 (branch intent), seam 8 (dispatch collapse and deletions), seam 9 (re-attach from `HostCodingAgents`) | — | ✅ |
| §1.3 Telegram's launcher | — (`feature/agent-tools/telegram-spec-launch`, n18) | — |
| §5 hangs: `mint_cursor_chat_id`, `write_slot_file_over_ssh` | ✅ deadlines | — |
| §5 fallback creep (`unwrap_or("claude-cli")`, hand-built arms) | — | ✅ |

Node names in Exploration 1 predate the decomposition: its "n13" is `feature/agent-tools/route-support` (#819); its "n15" is now `feature/agent-tools/headless-driver` / `spec-backend` (backend) and `feature/agent-tools/agent-config-removal` (config).

## Exploration 1

### n14 cli-spec-launch — discovery (read-only, base `0dbba9195`)

Scope: claude-cli and cursor-cli sessions (host PTY, tmux start/resume/re-attach, jail, recipes/managed workflows,
resume) launch through the generic spec path n8 built for opencode/muse/warp. No dispatch on
`CLAUDE_AGENT_ID`/`CURSOR_AGENT_ID`; branch intent from the spec. Includes the three `TODO(agent-tools-cascade)` markers
and `spec_launch::plan_host_pty`.
Sources: todo `docs/dev/todo/2026-11-04-claude-and-cursor-launch-by-agent-id.md`; older owners of the same debt:
`2026-11-02-claude-cli-hand-built-launch-removal.md` (names n12 as owner — not done), `2026-11-03-cursor-hand-built-launch-paths.md`,
`2026-11-03-harness-clis-legacy-cli-harness-removal.md`.

#### 1. State A

##### 1.1 Crates and edges
All launch code is in `tddy-agent-launch` (103 src files). Depends on (tddy-*): accounts, actions, cli-sessions,
coding-agent-specs, core, credentials, daemon-extensions, daemon-kernel, daemon-sandbox, discovery, host-service, pr-stack,
projects, rpc, sandbox, sandbox-recipes, sandbox-runner, semantic-index, service, session-activity, session-agents,
session-files, session-split, tool-engine, spawn, stdio, subagent-worktree, task, workflow, workflow-recipes, worktree-service.
Hand-built argv builders live in `tddy-cli-sessions` (`cli_session_manager/argv.rs:34 build_claude_argv`, `:64
with_claude_launch_flags`, `:107 build_cursor_argv`; `launch.rs:113,174,208 start_with_options`; `relaunch.rs:54`).
Integration suites live in `tddy-session-lifecycle/tests` (re-exports agent-launch).

##### 1.2 The dispatch (agent id, not session type)

| Symbol | Location |
|---|---|
| `CLAUDE_AGENT_ID`, `CURSOR_AGENT_ID` + `TODO(agent-registry)` | `tddy-agent-launch/src/harness_from_spec.rs:115-120` |
| `enum CliLauncher {Claude, Cursor, Generic}` | `harness_from_spec.rs:123-131` |
| `cli_launcher(agents, session_type)` (refuses no agents / no spec → FailedPrecondition) | `harness_from_spec.rs:136-157` |
| `recorded_cli_launcher(tddy_data_dir, st)` (loads `<data>/tools`) | `harness_from_spec.rs:161-171` |
| `launcher_of(id)` — the match on agent ids | `harness_from_spec.rs:173-179` |
| also: `spec_launch::CLAUDE_AGENT_ID` | `spec_launch.rs:389` (used :398, `claude_jail.rs:25,67,74`) |
| also: `cursor_from_spec::CURSOR_AGENT_ID` (pub) | `cursor_from_spec.rs:18` (used `cursor_cli_spawn.rs:16,392,489`) |

Dispatch sites:

| Site | Arms |
|---|---|
| `svc_start_session_core.rs:385` | `:388 Claude` → `cli_start_prelude` + `start_sandboxed_claude_cli_from_request` / `start_claude_cli_from_request`; `:427 Cursor` → `start_sandboxed_cursor_cli_from_request` / `spawn_cursor_cli_from_request`; `:466 Generic` → `cli_session_type` + `start_cli_harness` |
| `svc_resume_session.rs:58` | `:64 Claude` → `resume_claude_cli_session`; `:79 Cursor` → `resume_cursor_cli`; `:88 Generic` → `resume_sandboxed_cli_harness` / `resume_cli_harness` |
| `claude_cli_tmux.rs:299-317` (`reattach_recorded_main`) | `CliLauncher::Claude` → `reattach_recorded_claude` (:333) else `cli_harness_tmux::reattach_recorded_harness` (:242); **`unwrap_or("claude-cli")` at :294** — a default session type for a record without one |
| `cli_harness_tmux.rs:299-300` | `== Generic` check on re-attach |

##### 1.3 The bespoke claude path (what Generic lacks)

| Feature | Claude code | Generic (`opencode_cli_spawn.rs`) |
|---|---|---|
| entry | `cli_branch_starts.rs:117 start_claude_cli_from_request` → `svc_start_claude_cli_session.rs:34` → `claude_cli_spawn.rs:54 spawn_claude_cli_session_inner` (318 lines, code issue) | `cli_branch_starts.rs:286 start_cli_harness` → `opencode_cli_spawn.rs:139 start_cli_harness_session` |
| managed codebase / recipe | `placement::managed_recipe_for` (`svc_start_session_core.rs:398`), `claude_cli_spawn_steps.rs:250 managed_claude_cli_launch` | **refused**: `start_request_checks.rs:64-74 refuse_an_unsupported_cli_session_start` |
| hooks | `claude_cli_spawn_steps.rs:185 claude_cli_hooks`, `:207 install_claude_cli_hooks` (hand-built), spec `hooks` slot on spec path | `render_host_slots` passes `hooks: None` (`opencode_cli_spawn.rs:114-137`) |
| MCP/tool lists/system-prompt file | `spec_role_launch` → `AgentToolset` (`claude_cli_spawn_steps.rs:461`), `spec_launch.rs:392 launch_claude_cli` | all `None`/empty |
| session role (project home main) | `claude_cli_spawn_steps.rs:319 role_allowed_tools_args`, `:333 record_role_before_spawn`, `project_home_launch.rs:60 RoleWiring::hand_built` | none |
| operator notify | `notify_env::agent_notify_unless_role` (`claude_cli_spawn.rs:264`) | none |
| remote SSH checkout + slot files over ssh | `claude_cli_spawn.rs:169-185`, `spec_launch.rs:425-430 write_slot_file_over_ssh` (:300) | none |
| participant metadata / pr-stack link | `claude_cli_spawn.rs:193-205,344-385`, `hooks_and_urls.rs:81` | none |
| conversation id | daemon `session_id` (`--session-id`/`--resume`, claude.yaml run.argv) | `--continue` / warp token (`resume` slot) |
| tmux start | **hand-built**: `claude_cli_spawn_steps.rs:393-423` + `TODO(agent-tools-cascade)` :401 → `claude_cli_tmux.rs:61 start_claude_in_tmux` (`build_claude_argv` :81-93) | spec argv: `opencode_cli_spawn/harness_launch.rs:110-129` (`plan_host_pty` → `start_harness_in_tmux`) |
| tmux resume | **hand-built**: `svc_resume_claude_cli_session.rs:137-153` + `TODO(agent-tools-cascade)` :138 → `claude_cli_tmux.rs:354 relaunch_claude_cli` | `harness_launch.rs:122-123 resume_harness_in_tmux` |
| hand-built PTY fallback (no coding agents) | `claude_cli_spawn_steps.rs:414-418 spawn_hand_built` (:487) / `start_claude_in_pty` (:428); resume `svc_resume_claude_cli_session.rs:193-247` hand-built arm `resume_with_options` | refuses |
| jail | `claude_jail.rs:59 launch_jailed_claude` (spec-based, `jailed()` :162, `closure_defs` :171), `svc_start_sandboxed_claude_cli_session.rs:85` (615-line fn), `svc_resume_sandboxed_claude_cli_session.rs:12`, `svc_relaunch_sandboxed_runner.rs` | `cli_harness_jail.rs:53 launch_jailed_cli_agent` |
| branch intent | `BranchIntentPolicy::claude_cli()` at `claude_cli_spawn.rs:148`, `svc_start_sandboxed_claude_cli_session.rs:172` | `BranchIntentPolicy::cli_agent(kind.name, spec.has(BranchNameRequired))` `opencode_cli_spawn.rs:197-202`, `svc_start_sandboxed_cli_harness_session.rs:128-134` |
| other callers of the claude inner fn | `conversation_spawn_handler.rs:47`, `child_spawn_handler.rs:89` (pr-stack child), both pass `coding_agents` | — |

Cursor bespoke: `cursor_cli_spawn.rs:36 spawn_cursor_cli_session_inner` (337 lines), `:116 …_reporting`, `:363
spawn_cursor_from_spec` (spec path, tmux via `plan_host_pty` :435 + `HarnessTmuxLaunch::cursor` `cli_harness_tmux.rs:50`), dead
hand-built `:508 prepare_cursor_cli_launch`, `:551 spawn_cursor_cli_process` (`build_cursor_argv` :573, `#[allow(dead_code)]`
`TODO(cursor-cli)`); chat id `chat.rs:121 mint_cursor_chat_id`; resume `resume.rs:31 resume_cursor_cli_session`, `:166 chat_to_resume`;
jail `svc_start_sandboxed_cursor_cli_session.rs:66` (+ live `launch_jailed_cursor` :430, `write_jail_hooks` :516);
`BranchIntentPolicy::cursor_cli()` at `cursor_cli_spawn.rs:193`, `svc_start_sandboxed_cursor_cli_session.rs:146`.

Branch policy equivalence (`tddy-worktree-service/src/branch_intent.rs`): `claude_cli()` :63 = `cli_agent("claude-cli", true)` :83;
`cursor_cli()` :72 = `cli_agent("cursor-cli", false)`; `cli_harness(st)` :97 = `cli_agent(st,false)`. claude.yaml declares
`branch_name_required`; cursor.yaml does not. So the swap is behaviour-identical for the built-ins.

`spec_launch::plan_host_pty` (`spec_launch.rs:232-256`, pub): renders argv + `run.env`, writes slot files. Callers:
`opencode_cli_spawn/harness_launch.rs:111`, `cursor_cli_spawn.rs:435`, `resume.rs:128`, `launch_host_pty` :269. **Not** used by
claude tmux. `launch_claude_cli` (:392) writes slot files over ssh *before* calling `launch_host_pty`, which then writes them again
locally via `plan_host_pty` → for a remote host the local write is the bug fixed in `d2a096b14`; generic must keep the ssh branch.

`TODO(agent-tools-cascade)` at `harness_from_spec.rs:283-292 recorded_resume_token_adapter`: re-attach reads the spec from
`<tddy_data_dir>/tools` instead of `HostCodingAgents` (same for `recorded_cli_launcher` :161 and `require_sandbox_placement` :298).

Literal `"claude-cli"` (non-gate, would vanish with the bespoke path): `claude_cli_spawn_steps.rs:348`, `spec_launch.rs:440`,
`cli_branch_starts.rs:48,103,128`, `claude_cli_spawn.rs:338`, `claude_cli_tmux.rs:97,231,294`, `svc_relaunch_sandboxed_runner.rs:124`,
`jail_session_files.rs:70,107`, `project_home_start_spec.rs:21 READ_ONLY_SESSION_TYPE`, `start_request_checks.rs:103`;
`tddy-cli-sessions/src/cli_session_manager/{argv.rs:44, launch.rs:240}`.

Telegram has a **third** claude/cursor launcher, hand-built, outside agent-launch:
`tddy-telegram-control/src/telegram_session_control/cli_spawns.rs:118-135` (`config.claude_cli.binary_path`, `manager.start`),
`:345-370` (`install_cursor_hooks_in_worktree`, `resolve_cursor_binary_path`, `mint_cursor_chat_id`), `session_type` literals :160,:397;
`pickers.rs:254-255,355,360` compare session types (outside n12's scan).

##### 1.4 Counts

| What | Count |
|---|---|
| claude/cursor-specific launch files in agent-launch | 13 (`claude_cli_spawn{,_steps}.rs`, `claude_cli_tmux.rs`, `claude_jail.rs`, `svc_{start,resume}{,_sandboxed}_claude_cli_session.rs`, `cursor_cli_spawn.rs`, `cursor_from_spec.rs`, `svc_start_sandboxed_cursor_cli_session.rs`, `resume.rs`, `chat.rs`) ≈ 5,000 lines |
| generic counterparts | `opencode_cli_spawn.rs` (474) + `harness_launch.rs`, `cli_harness_tmux.rs` (358), `cli_harness_jail.rs` (357), `svc_{start,resume}_sandboxed_cli_harness_session.rs` |
| `CliLauncher::` matches | 9 (start 3, resume 3, re-attach 2, `launcher_of` 3 arms) |
| session-lifecycle suites on these paths | 20: `claude_cli_{from_spec,fullscreen_renderer,jail_from_spec,participant_metadata,permission_mode,session,tmux_reattach,tmux_session}_acceptance.rs`, `cursor_cli_{from_spec,hooks,resume_chat,session}_acceptance.rs`, `sandboxed_{claude_cli,claude_launch_baseline,cursor_cli,cli_harness}_acceptance.rs`, `cli_harness_tmux_acceptance.rs`, `harness_clis_from_spec_acceptance.rs`, `managed_codebase_workflow_acceptance.rs`, `managed_workflow_{transition,github_token}.rs`, `yaml_only_agent_acceptance.rs`; fixtures `claude_jail_support/`, `cursor_cli_support/`, `cli_harness_support/`, `harness_clis_support/`, `agent_registry_support/` |
| in-crate unit | `claude_cli_spawn_steps/claude_cli_spawn_steps_tests.rs`, `cli_harness_start_checks_unit_tests.rs`, `harness_from_spec.rs` tests |
| telegram suites | `telegram_start_claude_acceptance.rs`, `telegram_start_cursor_acceptance.rs` |

#### 2. Seams and order

The generic path must first *grow* every feature the bespoke paths have, each keyed on a spec slot or capability; then the dispatch collapses.

1. **Template params superset.** `harness_template_params` (`harness_from_spec.rs:52`) binds also `session_id`, `permission_mode`
   (default `auto`), `dangerously_skip_permissions`, `chat_id`; replaces `claude_template_params` (`spec_launch.rs:452`) and
   `cursor_template_params` (`cursor_from_spec.rs:56`). Template renders only what it names (opencode/muse/warp unchanged).
2. **Conversation id from the spec.** New `coding_agent.conversation` (or extend `ResumeSlot`): `daemon_session_id` (claude),
   `minted: { adapter: cursor-create-chat }` (cursor, new `LaunchAdapters` entry wrapping `chat::mint_cursor_chat_id`), `worktree`
   (opencode/muse), `token` (warp, existing `resume.adapter`).
3. **Slots on the generic host launch.** `render_host_slots` fills hooks (when `spec.has(Hooks)`), MCP/tool lists/system-prompt file
   from the role toolset, `home`. The claude role/notify/ssh steps move into generic helpers gated by `ProjectHomeMain`,
   `OperatorNotify`, and a new capability for remote checkout (or keep ssh generic — open).
4. **Managed workflow.** New `Capability::ManagedWorkflow` (claude, cursor); `refuse_an_unsupported_cli_session_start` refuses only
   when absent; `managed_claude_cli_launch` → generic. (Capability enum is closed — adding one is a spec-crate change + 9 YAMLs.)
5. **tmux.** Claude tmux start/resume use `plan_host_pty` → `start_harness_in_tmux`/`resume_harness_in_tmux` (closes both cascade
   TODOs); delete `start_claude_in_tmux`, `relaunch_claude_cli`, `reattach_recorded_claude` and the `unwrap_or("claude-cli")`.
6. **Jail.** `launch_jailed_claude` / `launch_jailed_cursor` fold into `launch_jailed_cli_agent` (closure defs + withdrawn tools are
   spec-driven already).
7. **Branch intent.** `claude_cli()`/`cursor_cli()` callers → `cli_agent(kind.name, spec.has(BranchNameRequired))`; delete the two
   constructors and `cli_harness`.
8. **Collapse dispatch.** `cli_launcher` → `Option<CliSessionType>`; delete `CliLauncher`, `launcher_of`, both `*_AGENT_ID`s,
   hand-built arms (`spawn_hand_built`, `start_claude_in_pty`, `resume_with_options` arm, `spawn_cursor_cli_process`,
   `prepare_cursor_cli_launch`), and `CliSessionManager::{build_claude_argv, with_claude_launch_flags, build_cursor_argv,
   start_with_options, start_cursor, resume_cursor}` once no caller remains (split agent `tddy-session-split/src/svc_spawn_split_agent.rs:262`
   still calls `start_with_options` — keep or move under n14's scope; ASK).
9. **Re-attach registry source** (cascade TODO :283): pass `HostCodingAgents` into `reattach_recorded_main` instead of loading `<data>/tools`.

Each step keeps the tree compiling if done in this order (1-7 additive, 8 deletes). Blocking: none upstream; the `#[allow(dead_code)]`
cursor items and the claude hand-built argv need deletion approval (CLAUDE.md § ASK) — the two older todos already list them.

#### 3. Proposed owned API surface

```rust
// tddy-coding-agent-specs
pub enum Capability { …, ManagedWorkflow }                 // claude, cursor
pub struct ConversationSlot { kind: ConversationKind, adapter: Option<String> }   // coding_agent.conversation
pub enum ConversationKind { DaemonSessionId, Minted, Worktree, ResumeToken }
// tddy-agent-backend::adapters
pub trait ConversationIdAdapter: Send + Sync { fn mint(&self, binary: &Path, workdir: &Path, deadline: Duration) -> BoxFuture<Result<String, String>>; }
impl LaunchAdapters { pub fn conversation_id(&self, name: &str) -> Option<Arc<dyn ConversationIdAdapter>>; }  // "cursor-create-chat"
// tddy-agent-launch
pub(crate) fn cli_launcher(agents: Option<&HostCodingAgents>, st: &str) -> Result<Option<CliSessionType>, Status>;
pub(crate) struct HarnessLaunch<'a> { model, prompt, resume, resume_token, session_id, conversation_id, permission_mode, dangerously_skip_permissions }
pub(crate) struct HostLaunchSlots<'a> { toolset: AgentToolset, hooks: Option<HookCommandParams<'a>>, system_prompt_file: Option<&'a Path>, remote_host: Option<&'a str> }
pub(crate) async fn start_cli_agent_session(start: CliAgentStart<'_>) -> Result<Response<StartSessionResponse>, Status>; // replaces start_cli_harness_session + spawn_claude_cli_session_inner + spawn_cursor_cli_session_inner
pub async fn reattach_recorded_main(manager, agents: &HostCodingAgents, os_user, session_id) -> bool;
```

Removed: `CliLauncher`, `launcher_of`, `CLAUDE_AGENT_ID` ×2, `CURSOR_AGENT_ID`, `launch_claude_cli`, `claude_template_params`,
`cursor_template_params`, `start_claude_in_tmux`, `relaunch_claude_cli`, `reattach_recorded_claude`, `spawn_hand_built`,
`start_claude_in_pty`, `install_claude_cli_hooks`, `prepare_cursor_cli_launch`, `spawn_cursor_cli_process`,
`BranchIntentPolicy::{claude_cli, cursor_cli, cli_harness}`, `recorded_cli_launcher`, `recorded_resume_token_adapter`; the
hand-built `CliSessionManager` builders (ASK). Optionally `launch_jailed_claude`/`launch_jailed_cursor`.
Kept: `spec_launch::plan_host_pty` (now the single argv source for PTY and tmux).

#### 4. Candidate acceptance tests

| # | Test | File | Fixture |
|---|---|---|---|
| B1 | `a_dropped_in_agent_declaring_hooks_and_managed_workflow_starts_over_a_managed_codebase_with_its_hooks_file_written_and_the_recipe_prompt_appended` | `tddy-session-lifecycle/tests/yaml_only_agent_acceptance.rs` | `agent_registry_support` (`a_daemon_with_tools`, stub report), `managed_codebase_workflow_acceptance.rs` recipe setup |
| B2 | `a_claude_cli_start_in_tmux_runs_the_argv_the_claude_spec_renders` (byte-identical to the PTY start's argv) and the same for resume | `claude_cli_tmux_session_acceptance.rs` | existing tmux fixture + `builtin_coding_agents()` |
| B3 | `a_host_without_coding_agents_refuses_a_claude_cli_and_a_cursor_cli_start_failed_precondition_spawning_nothing` | `claude_cli_session_acceptance.rs` / `host_without_integrations.rs` | `test_host` without `with_coding_agents` |
| B4 | `removing_branch_name_required_from_an_override_of_claude_yaml_lets_a_blank_new_branch_name_generate_one` | `yaml_only_agent_acceptance.rs` (beside `removing_hooks_from_an_override_…` :94) | `claude_yaml_without_hooks()` pattern |
| B5 | `a_dropped_in_agent_whose_conversation_is_the_daemon_session_id_resumes_with_the_same_id` (stub reports argv on start and resume) | `yaml_only_agent_acceptance.rs` | `agent_registry_support` |
| B6 | `a_daemon_restart_reattaches_a_tmux_claude_cli_session_from_the_installed_coding_agents` (not `<data>/tools`) | `claude_cli_tmux_reattach_acceptance.rs` | existing |

Plus the scan: extend `no_agent_name_gates.rs` with agent-id literals (`"claude"`, `"cursor"`) compared in `tddy-agent-launch` (pins "no dispatch on agent id").
Unit: `harness_template_params` binds `session_id`/`permission_mode` default `auto`; `ConversationKind` parse/refuse unknown adapter at load;
`cli_launcher` returns the session type for every built-in and `None` for `tool`/`workspace`; `BranchIntentPolicy::cli_agent` equals the
removed constructors (one-off equivalence test before deletion).

#### 5. Risks

- **Hangs (no timeout today):** `chat::mint_cursor_chat_id` (`chat.rs:121-138`) `Command::output()` with no deadline — becomes the
  `cursor-create-chat` adapter; give it a deadline + refusal naming the agent. `spec_launch.rs:300 write_slot_file_over_ssh` is a
  blocking `wait_with_output()` on `ssh` inside an async launch, no timeout. tmux pane start `start_agent_pane` (`claude_cli_tmux.rs:118`) runs
  in `in_tmux_blocking`; check its bound. Re-attach `agent_tmux_session_is_alive` before a stale pane.
- **Fallback creep:** `claude_cli_tmux.rs:294 unwrap_or("claude-cli")` — a record with no session type re-attaches as claude; must refuse/skip.
  `spawn_claude_cli_process` `_ =>` arm (`claude_cli_spawn_steps.rs:414`) runs hand-built when `coding_agents` is `None` — exactly the
  fallback rule; delete, refuse FailedPrecondition. `relaunch_claude_cli` hand-built arm (`svc_resume_claude_cli_session.rs:230`) likewise.
- **Behaviour drift:** hand-built and spec argv are kept byte-identical by hand today (todo 2026-11-02); the tmux path is the one place they
  may differ — B2 pins it. Claude `--session-id` vs `--resume` toggle must survive the generic `resume` flag.
- **Linux/macOS jail:** `launch_jailed_claude` writes a jailed spec (`claude_jail.rs:162 jailed`) and closure of `tddy-tools`; Linux jail is not
  run on CI (`docs/dev/todo/2026-11-02-ci-runs-no-linux-jail.md`); macOS-only `jailed::` tests as in yaml_only. Persistent homes
  (`home: { kind: persistent, key: claude|cursor }`) already spec-declared.
- **Proto/web:** none needed (`StartSessionRequest.run_in_tmux`, `managed_codebase` unchanged). Web picker already reads capabilities via
  `AgentInfo`. If `ManagedWorkflow` should hide the managed toggle for agents without it → `tddy-web` change (optional).
- **Scope traps:** Telegram's own launcher (`cli_spawns.rs`) is a third claude/cursor path — in n14 (route it through `LaunchSessions`)
  or left; it also reads `config.claude_cli` (n15's removal). `conversation_spawn_handler.rs`/`child_spawn_handler.rs` hard-code a claude child.
  Split-session host agent (`2026-11-03-split-sessions-render-the-embedded-agent-spec.md`) is separate.
- **Docs at wrap:** `docs/ft/daemon/claude-cli-session.md`, `cursor-cli-session.md`, `opencode-cli-session.md` (shared launch), `session-agent-roster.md`,
  `docs/ft/coder/managed-codebase-workflow.md`, `packages/tddy-agent-launch/docs/module-layout.md`, `packages/tddy-coding-agent-specs/docs/coding-agent-specs.md`
  (conversation slot, new capability). Close todos `2026-11-04-claude-and-cursor-launch-by-agent-id.md`, `2026-11-02-claude-cli-hand-built-launch-removal.md`,
  `2026-11-03-cursor-hand-built-launch-paths.md`.

#### 6. Overlap

| With | Edge |
|---|---|
| n13 | 5 `Harness::ClaudeCli` route call sites in the claude path (`claude_cli_spawn.rs:413`, `cli_branch_starts.rs:49,110`, `svc_resume_claude_cli_session.rs:297`, `svc_resume_sandboxed_claude_cli_session.rs:51`). With n13 first they are spec-keyed and fold into `cli_route_launch` unchanged; with n14 first, n14 would carry `Harness::ClaudeCli` into the generic path and n13 rewrites it again. **n13 before n14.** |
| n15 | n14 deletes most readers of `config.claude_cli`/`cursor_cli` in the claude/cursor paths (`spec_launch.rs:475 configured_binary`, `cursor_from_spec.rs:22`, `claude_cli_spawn_steps.rs:188-198`, `svc_resume_claude_cli_session.rs:108`, `svc_start_sandboxed_claude_cli_session.rs:284`, `svc_relaunch_sandboxed_runner.rs:129`), shrinking n15. If n15 went first it would edit code n14 deletes. **n14 before n15.** Fixture migration (n15) touches the same 20 suites n14 rewrites → merge-conflict risk; n14 should leave `daemon_config_from_fixture_file` callers alone. |
| `coding_agents: Option<…>` | making it non-optional needs every fixture to call `with_coding_agents` — that is n15's fixture sweep; n14 refuses at runtime instead. |

#### 7. Complexity

**5** — several row-4 signals: many crates (agent-launch, coding-agent-specs + 9 YAMLs, agent-backend adapters, worktree-service,
cli-sessions, session-lifecycle tests, possibly telegram-control); spawned-process + tmux I/O seams; ordering of tmux re-attach after restart;
inherited restructure debt (5 code-issue records on the files it rewrites); unsettled design (conversation slot shape, ssh as capability or
generic, Telegram in/out). **Recommend splitting**: n14a claude (tmux, managed, role, notify, ssh, hooks) and n14b cursor (conversation-id
adapter, jail fold, dead hand-built deletion), or n14a "generic launch grows slots + capabilities" then n14b "dispatch collapse + deletions".

#### 8. Code issues touched

| Record | Metric | Claimed |
|---|---|---|
| `tddy-agent-launch/docs/code-issues/complexity-svc-start-session-core-start-session-core.md` | 842 lines, CRAP 49 | unclaimed |
| `complexity-claude-cli-spawn-spawn-claude-cli-session-inner.md` | 318 lines | unclaimed |
| `complexity-cursor-cli-spawn-spawn-cursor-cli-session-inner.md` | 337 lines, CRAP 930 | unclaimed |
| `complexity-svc-resume-claude-cli-session-resume-claude-cli-session.md` | 142 lines, 9 params | unclaimed |
| `complexity-svc-start-sandboxed-claude-cli-session-start-sandboxed-claude-cli-session.md` | 615 lines, CRAP 2862 | unclaimed |
| `crap-svc-start-sandboxed-cursor-cli-session.md`, `oversized-file-svc-start-sandboxed-cursor-cli-session.md` | CRAP 1722 / 549 lines | unclaimed |
| `oversized-file-cursor-cli-spawn.md` | 587 lines | unclaimed |
| `complexity-claude-jail-launch-jailed-claude.md` | 80 lines, deferred to the #agent-tools sweep | unclaimed |
| `complexity-opencode-cli-spawn-start-cli-harness-session.md`, `…-resume-cli-harness-session.md`, `complexity-cli-branch-starts-start-cli-harness-from-request.md` | the generic path that grows | unclaimed |
| `tddy-worktree-service/docs/code-issues/complexity-branch-intent-resolve-branch-workflow.md` | 86 lines | unclaimed |

None `Claimed by:`. Deleting the bespoke paths should close several; re-measure at wrap.
