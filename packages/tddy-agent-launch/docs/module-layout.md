# `tddy-agent-launch`: module layout

The agent launch: how a session starts, resumes and relaunches, and how a started session spawns children.
The crate holds the topic code of what the daemon runs as `session.SessionService`'s start, resume, list,
connect, signal and delete handlers; `tddy-session-lifecycle` wires it to the host
([module-layout](../../tddy-session-lifecycle/docs/module-layout.md)) and re-exports it at the old
`tddy_session_lifecycle::…` paths.

About **9.5k production lines** in 55 modules, under the 10k receiver budget. No production file is over 500 (the largest, by the count to the first `#[cfg(test)]`:
`svc_start_session_core.rs` 486, `session_coordinate_handlers.rs` 460).
Standing findings are in [`code-issues/`](code-issues/). A production line is any line of a `src/` file
outside an inline `#[cfg(test)] mod x { … }` block, excluding `*_tests.rs`.

## Launch sessions

`launch_ports.rs` defines `LaunchSessions`, an owned handle over the launch fields of the daemon host
(`config`, `tddy_data_dir`, `staging_base_dir`, `peer_routing`, `claude_cli_manager`, `sandbox_manager`,
`task_registry`, `session_stdio`, `agent_activity_hub`, `user_resolver`, `spawn_client`,
`workspace_sandboxes`, `rpc_activity`, `session_agent_inference`, `session_rooms`, `hosted_agent_clones`,
`session_admissions`, `worktree_observer`, `session_reach`, `reachable_sessions`, `host_session_sockets`, `agent_tool_sockets`, and the agent topic's `AgentRoster` and
the split topic's `SplitSessions` handles), and `trait LaunchHost`, the four callbacks the host implements:
`sandbox_rpc_handler` (the dispatch a jail relays family B to, bound to a session), `pr_stack` (the PR-stack
handler, or `FAILED_PRECONDITION` when the host has no RPC families), `session_account_access` (what a
session's vault reads go through) and `session_identity` (the commit pairs and the `github-token` handler a
session is launched with). The seed-clone claimant is not a callback: it holds the roster handle.

The launch methods are `impl LaunchSessions` in the files they are declared in. A task a method spawns
clones the handle, as do the child-spawn handler, the conversation-spawn handler and the host-session
socket. `impl StackParentHost` is on the handle.

## Session start

`svc_start_session_core.rs` holds `start_session_core`, the one implementation behind `StartSession` and
`StreamStartSession`: it checks the request, routes it, and dispatches by placement and session type.
`start_session_on_request` decides the route and the placement, refuses an orchestrated `tool` start
(`start_request_checks::refuse_an_orchestrated_tool_start` — a `tool` session's changeset is written by
its own `tddy-coder`), then calls `start_here` (the per-session-type start). When the request names an
orchestrator (`orchestrator_session_id`, and `orchestrator_daemon_instance_id` when it is another
daemon), `svc_start_session_core/orchestrator_record.rs` (`LaunchSessions::record_orchestrator`) writes
it into the new session's changeset once the start has returned — on the daemon that created the
session, so a start forwarded from the orchestrator's host is linked where it lives. Its steps are in
sibling modules:

| Module | Holds |
|---|---|
| `harness_profile_request.rs` | `LaunchSessions::start_session_core` is a wrapper: it applies the [harness profile](../../../docs/ft/daemon/harness-profiles.md) named by `req.agent` (looked up in `<tddy_data_dir>/agents`) before the allowed-agents check and peer forwarding, then calls `start_session_on_request` (`pub(crate)`, in `svc_start_session_core.rs`). `start_session_core` refuses a requested role with `PERMISSION_DENIED` (`refuse_a_requested_role`) and applies the profile; `start_session_with_role` (`pub(crate)`) takes a request whose profile is already applied — `EnsureProjectHomeSession` is its one caller, and applies the profile itself (`apply_harness_profile`, `pub(crate)`) so it can check the session type the profile resolves to. `request_on_profile` puts the profile's account at the `agent_definition` level beside the request's own session-level choice, runs `choose_route` once, resolves the winner in the starter's vault and writes its **id** into `provider_account_id`; the profile's model is used only when the request gives none. Refusals name the profile and the account |
| `harness_profile_start.rs` | `profile_start(&HarnessProfile) -> ProfileStart { session_type, agent, choice: RouteChoice::Account, model }`: a CLI harness (`claude-cli`, `opencode-cli`, `muse-cli`, `warp-cli`, `cursor-cli`) becomes the session type, a tool-session harness (`claude`, `claude-acp`, `codex`, `codex-acp`, `opencode`, `cursor`) the agent; any other is `INVALID_ARGUMENT` |
| `provider_account_lookup.rs` | `find_provider_account`, behind `SessionAccountAccess::provider_account`: an id first, then a unique label; none is `NOT_FOUND`, a label on two accounts `SessionIdentityRefusal::AmbiguousLabel` (`INVALID_ARGUMENT`), never a guess |
| `start_request_checks.rs` | `refuse_a_cli_start_without_a_model` (an agent whose spec declares `requires_model` needs a model; shared by `start_session_on_request` and the project-home up-front check), `refuse_a_conflicting_tmux_run_start` (a `run_in_tmux` of any CLI agent session type — `runs_in_tmux`, the `*-cli` suffix — with `sandbox`, `sandboxed_codebase` or a split placement is `invalid_argument` naming the conflict; called by `start_session_core_cancellable` after the harness profile, before routing), `refuse_an_unsupported_cli_session_start(spec, session_type, req)` (a generic CLI agent start over a managed codebase is `invalid_argument` naming the agent unless its spec declares `managed_workflow`; tests in `managed_workflow_tests`), `forward_start_session` (a start owned by a peer, forwarded through `forward_start_session_relaying` into a discarding sink — no deadline; bounded by the peer's keep-alives and the relay's idle timeout), `provision_project_for_start` (keeps a streamed start alive while it waits), `validate_stack_seed_against_project` |
| `workspace_branch_start.rs` | `seed_and_start_workspace_session` |
| `cli_branch_starts.rs` | the start of every CLI agent — claude-cli, cursor-cli and every other `*-cli` spec alike: `cli_start_prelude` (sessions base, new id, attachments and initial prompt, returned as a `CliStart`); `start_cli_harness(kind, …)` (the agent resolved from its spec first, with no side effect — `resolve_session_agent`, `FAILED_PRECONDITION` without coding agents; then `refuse_an_unsupported_cli_session_start` over a managed codebase and, for a sandboxed start, `require_sandbox_placement` — the sandboxed `warp-cli` refusal — before any provider route is resolved; then the prelude; then the jailed start when `sandbox` is set and the PTY start otherwise); `refuse_a_claude_cli_start_up_front` (the refusals an unsandboxed CLI start would answer before it creates anything — model, caller-chosen id, recipe via `managed_recipe_for`, the agent the session type names, provider route — for a caller that must stop something first); `start_spawned_cli_child(SpawnedChild)` (a conversation or pr-stack child, started through `opencode_cli_spawn::start_cli_harness_session` as its **parent's** agent — `harness_from_spec::child_session_type` of the parent's recorded session type; a host without coding agents, a parent recording no session type or one that is no CLI agent's is `FAILED_PRECONDITION` before anything is cut) |
| `cli_session_worktree.rs` | what a daemon-cut CLI session's worktree needs, shared by every CLI agent's host and jailed start: `provision_cli_worktree(&CliWorktreeStart, …) -> CliWorktree { path, project }` (cut or adopt the worktree, push the new branch when asked) and `semantic_index_env` |
| `opencode_cli_spawn.rs` | the host start and resume of the [OpenCode](../../../docs/ft/daemon/opencode-cli-session.md), [Muse Code](../../../docs/ft/daemon/muse-cli-session.md) and [Warp](../../../docs/ft/daemon/warp-cli-session.md) sessions, from `opencode.yaml` / `muse.yaml` / `warp.yaml`: `start_cli_harness_session` (route first, then worktree on `harness_from_spec::cli_branch_intent`, seeded clones, commit identity, the spec rendered with `harness_template_params` and launched through `launch_host_pty`, with the spec's resume adapter as its `on_exit` hook when it declares one, `.session.yaml`, the [session pointer](#the-cli-spawns) placed in the worktree, then the recorder armed) and `LaunchSessions::resume_cli_harness` (a tmux-run session first `cli_harness_tmux::settle_unwatched_agent_exit`, then `settled_resume_token`, then the spec's resume argv: `resume` for `--continue`, `resume_token` for Warp; route re-resolved; refused when the worktree is gone; the pointer written again). `start_session_core` and `resume_session_at_session_coordinate` reach it for every `*-cli` session type through `harness_from_spec::cli_launcher` — a sandboxed session through `svc_{start,resume}_sandboxed_cli_harness_session` instead. It is the one CLI launch: claude-cli and cursor-cli start, resume and re-attach through it, `claude.yaml`'s and `cursor.yaml`'s needs included, through `cli_agent_steps`. `opencode_cli_spawn/resume.rs` holds the resume (`LaunchSessions::resume_cli_harness`, `resume_cli_harness_session`, `refuse_a_resume_without_its_worktree`). `opencode_cli_spawn/harness_launch.rs` is the launch both take: `launch(cli_manager, HarnessSession, HarnessSpecLaunch, HarnessRun)` — the spec through `launch_host_pty`, or, for a tmux-run start or resume (`#tmux-agent` 3/3), `plan_host_pty`'s argv through `cli_harness_tmux::start_harness_in_tmux` / `resume_harness_in_tmux` |
| `cli_agent_steps.rs` | the generic CLI launch's spec-keyed steps (`#agent-tools 14/19`): `render_host_launch_slots(HostLaunchSlots)` (hooks, MCP / tool-list from the role's `AgentToolset`, the appended system prompt, `{home}`), `write_slot_files_on_target` (an ssh checkout's slot files through `slot_file_over_ssh`, `DEADLINE_EXCEEDED` naming the host), `start_conversation` / `resumed_conversation` / `resumed_or_adopted_conversation` (a record without the conversation its spec mints adopts a fresh one, recorded for every later resume) → `LaunchConversation::{SessionId, Minted, Worktree, ResumeToken}` (`bound`, `recorded_chat_id`; a mint past its deadline is `conversation_mint_refused`), `refuse_a_role_the_agent_cannot_hold` (`project_home_main`), and `wire_session_agent` / `LaunchSessions::resumed_session_wiring` (hooks and operator notify for a spec declaring them, the managed workflow for `managed_workflow`, a main's tool restriction; every `TDDY_*` pair they produce goes to the tool server's `env` or the session pointer, never the agent process). The conversation steps (`LaunchConversation`, `ConversationStart` and the functions from `start_conversation` to `refuse_a_role_the_agent_cannot_hold`) live in `cli_agent_steps/conversation.rs`. Tests in `cli_agent_steps/cli_agent_steps_tests.rs` |
| `harness_from_spec.rs` | what every CLI agent spec binds: `OPENCODE_AGENT_ID` / `MUSE_AGENT_ID` / `WARP_AGENT_ID`, `HarnessLaunch` → `harness_template_params` (`model`, `prompt`, `resume`, `resume_token`, the conversation's `session_id` / `chat_id`, `permission_mode`, `dangerously_skip_permissions`, `node_launcher` / `node_entry`, blanks dropped; a resume by id re-binds the model, a resume by worktree or token binds neither model nor prompt), `require_spec_placement` (`FAILED_PRECONDITION` naming the spec's placements — the sandboxed `warp-cli` refusal), `cli_launcher(agents, st) -> Result<Option<CliSessionType>, Status>` (`None` for a type that is no `*-cli` agent's; a host with no coding agents or no spec for the type is `FAILED_PRECONDITION`; nothing reads which agent it is), `recorded_cli_session_type` (the re-attach's source: `None` for a record without a session type, never a default agent), `child_session_type` (a conversation or pr-stack child runs its parent's agent), `cli_branch_intent(kind, spec)` (`BranchIntentPolicy::cli_agent(session type, spec.has(BranchNameRequired))`, the one branch-intent source of every CLI start), `cli_session_type(agents, st)` (session type → spec id and route harness, both interned names; no spec is an error, never a hand-built launch), `resolve_session_agent` (`FAILED_PRECONDITION` without coding agents), `resume_token_adapter`, `recorded_resume_token_adapter` (the adapter a tmux re-attach reads from the host's installed coding agents) and `LaunchSessions::require_sandbox_placement`. Unit tests in `harness_from_spec/cli_agent_dispatch_unit_tests.rs` |
| `cli_resume_token.rs` | the resume token a CLI agent prints as it exits (Warp's `warp --resume <TOKEN>`): `resume_token_recorder(session_dir, adapter) -> (ExitHook, ResumeTokenArm)` — the hook the host launch passes as `on_exit`, run once the process has exited and its output is complete, before the terminal is deregistered; it reads the capture through the spec's `warp-resume-token` adapter and writes only `harness_resume_token`, only when a token was found. It waits for `ResumeTokenArm::arm`, which the start and the resume fire once they have written the whole metadata; a dropped arm records nothing. `settled_resume_token` waits on `CliSessionManager::exit_hook_settled` and re-reads the token. Only a spec with a `resume` adapter gets the hook |
| `tool_session_spawn.rs` | `spawn_tool_session` and `spawn_tddy_coder`: the one `tddy-coder` spawn, for both a start and a resume |
| `tool_session_route.rs` | a tool session's provider route as route-file JSON: `tool_session_route_json`, `resumed_tool_session_route_json`; see [Provider route of a session](#provider-route-of-a-session) |
| `tool_spawn_plan.rs` | `ToolSpawnPlan` (what the child is spawned with, including `provider_route_json`) and `ToolSpawnPurpose` (`Start` / `Resume`) |
| `svc_ensure_project_available_for_start.rs`, `svc_index_workspace_worktree.rs` | project provisioning for a start; the semantic index over a workspace worktree |
| `svc_ensure_project_home.rs` | `EnsureProjectHomeSession`: forwards a request naming another daemon, then holds a process-wide lock **before** reading the project, so two set-ups cannot both find no main; `caller_home_scope` (OS user, sessions base, projects dir, registered project); `recorded_main` (the session `ProjectData.home_session_id` names — cleared when its directory is gone, `INTERNAL` when its record is unreadable). Without `start`, `reach_recorded_main`: none → `NOT_FOUND`, running (pid alive or terminal served) → unchanged, stopped → `fast_forward_home` + `resume_session_at_session_coordinate`. With `start`, `set_up_main`: `refuse_what_main_cannot_honour`, `apply_harness_profile`, `refuse_a_type_main_cannot_run`, `home_worktree_for` (`ensure_home_worktree`), `on_home_worktree`, `refuse_a_claude_cli_start_up_front` — every check before anything stops; then `stop_previous_main` (fast-forward, stop a served recorded main) and `start_and_record_main` (`start_session_with_role`, then `project_storage::set_project_home_session`; a main that cannot be recorded is stopped again). `EnsureOutcome`: `started` / `replaced` / `resumed` / `running` |
| `project_home_start_spec.rs` | the start request main runs from: `refuse_what_main_cannot_honour` (a requested `session_role` → `PERMISSION_DENIED`; `sandboxed_codebase`, `codebase_daemon_instance_id`, `ssh_config_host`, `agent_clone`, `split_agent` → `INVALID_ARGUMENT`), `refuse_a_type_main_cannot_run` (anything but `claude-cli`, checked after the profile is applied → `FAILED_PRECONDITION`), `on_home_worktree` (sets project, home-worktree `repo_path`, `managed_codebase`, the `project-home` role; clears branch, stack and `daemon_instance_id` fields; keeps the rest of the spec) |
| `claude_cli_spawn_steps.rs` (role) | `record_role_of_session_type_before_spawn` writes a session's role into its initial `.session.yaml` before the agent is spawned (nothing for an ordinary session); the generic CLI launch and `launch_jailed` call it |
| `project_home_launch.rs` | what a session's role adds to its claude-cli agent's launch. `StartRole { name, main: Option<MainLaunchPlan> }` replaces a bare role name on the spawn (`StartRole::ordinary()` for every caller with no role); `LaunchSessions::start_role` and `resumed_role` mint a main's agent credential (`SplitSessions::agent_session_token_for`, from the verified caller) through `main_plan` and nothing for any other role. `MainLaunchPlan::launch` is the one place that resolves the toolchain's `tddy-tools` and calls `project_home_wiring`. That writes the role prompt (`project_home_prompt.md`, `include_str!`) into the session dir and returns a `RoleWiring { main: Option<MainAgentFlags>, env }`, where `env` is the `colocated_jail_tool_env` pairs (agent-tool socket on an embedded host, local URL otherwise). A main's `MainAgentFlags` (the role's allowed tools, every `NATIVE_FILESYSTEM_TOOLS` entry withdrawn, `tddy-tools --mcp` as its only MCP server, the role prompt) render through `MainAgentFlags::spec_launch` / `spec_role_launch`: an `AgentToolset` for the `claude` spec's slots, plus the role prompt flag. A start's plan is carried out by the generic CLI start (`cli_branch_starts.rs`), after every check, so a refused start writes nothing. A resume carries it out in `resumed_role` (`cli_agent_steps::resumed_session_wiring`) |
| `clone_planner.rs`, `clone_backend.rs`, `clone_ssh_env.rs`, `clone_stopper.rs`, `start_keepalive.rs` | how a clone reaches its remote, runs and is stopped; see [Cloning a project](#cloning-a-project-for-a-start) |
| `start_task.rs` | the streamed start as a task; see [A streamed start is a task](#a-streamed-start-is-a-task) |
| `svc_start_claude_cli_session.rs` | what a CLI start binds to a managed session's toolcall listener: `managed_spawn_handlers` (a pr-stack orchestrator's child spawner, a grill-me session's conversation spawner, neither for any other recipe), `conversation_spawn_handler_for`, and the tool session's host-socket registration (`register_tool_session_on_host_socket`, `ToolSessionHostRegistration`, `tool_session_spawned`) |
| `svc_resume_claude_cli_session.rs` | `resumed_recipe`: the recipe a resume prepares a managed workflow for — none when the agent is live in tmux and this daemon still serves its workflow (a new listener would unlink the socket the agent uses). Every CLI resume is `opencode_cli_spawn::LaunchSessions::resume_cli_harness`, or `resume_sandboxed_cli_harness` for a sandboxed session |

### The start deadline

`start_here` runs its body (`start_here_body`) under `start_deadline::StartDeadline::new(config.session_start_timeout())`.
Both entry points reach it: `start_session_core_cancellable` → `start_session_with_role` →
`start_session_on_request` → `start_here`, and `start_session_for_os_user` → `start_here`.

`StartDeadline::run(progress, cancel, start, undo)` hands `start` a sink of its own and reads that
sink's channel **in the task that polls the start** (`Relay`), so the begun-and-not-ended phase it
names at expiry is exact; a forwarding task would lag the timer. It races `sleep_until` against the
start rather than wrapping it in `tokio::time::timeout`, because the grace (`START_CANCEL_GRACE`, 30 s,
`with_cancel_grace` for tests) must keep polling the same future after the deadline. On expiry: cancel
the token, poll for the grace, `undo` a session that came back, drop a start that did not, drain the
relay, return `DEADLINE_EXCEEDED`. `undo` returns `()`: its failure is logged, not returned.

The undo is `start_task::undo_a_timed_out_start`, keyed by OS user because an operator start carries
no token: `remove_what_a_start_created` reads the cut branch (`branch_a_start_cut_as`), deletes the
session (`delete_session_as`, the body of `DeleteSession` after `os_user_of_session_token`), then the
branch. `undo_a_cancelled_start` (the streamed start's cancel) resolves its token with
`os_user_of_session_token` and runs the same teardown. An operator start's peer-held parts (split
codebase half, agent clones) are deleted with its empty token and so refused, leaving the session in
place — `TODO(operator-start-undo)` in `start_task.rs`,
`docs/dev/todo/2026-11-07-operator-start-undo-of-peer-held-parts.md`.

The wiring tests are in `tddy-session-lifecycle/tests/session_start_deadline_acceptance.rs` (the
`DaemonSessionHost` harness cannot be a dependency of this crate); their stuck step is an SSH clone to
192.0.2.1 (TEST-NET-1), as in `start_cancel_acceptance`.

## Cloning a project for a start

`ensure_project_available_for_start` clones the project's checkout when the host has none, with no
deadline. The cloner it hands `tddy-projects` (`clone_as_a_start_step`, in
`svc_ensure_project_available_for_start.rs`) captures the start's `AttachmentProgressSink`: it begins
`STEP_CLONE`, feeds each line of git's progress into the sink as a `CloneProgress` event
(`clone_backend::clone_progress_event`), and ends the step when the clone succeeds — so the
`Fn(&str, &Path) -> Result<(), String>` cloner signature in `tddy-projects` carries no progress parameter.
How the clone reaches its remote is decided per clone, on the blocking pool, where the URL being cloned is
known:

| Module | Holds |
|---|---|
| `clone_planner.rs` | `ClonePlanner` and `PlannedClone`: a facilitator clone runs with only its transport shim; an SSH remote runs with the askpass environment when the host's OpenSSH is 8.4 or later and an agent is reachable; otherwise it runs without prompting (`BatchMode=yes`) and a failure says why. The grant a clone holds lives until it ends. A refusal before the clone starts (an unresolvable session) is kept and returned as its own `Status`. |
| `clone_ssh_env.rs` | `ClonePlan::{Facilitator, Ssh}`, `clone_ssh_env_for`, `agent_socket_for` and `CloneAgentError` (`NoAgentReachable` and `LookupFailed`, kept distinct), `is_ssh_url`, `resolve_askpass_path` (beside the daemon executable, then `PATH`) |
| `clone_backend.rs` | `CloneBackend::clone_repo(url, dest, env, on_started, on_progress)`: the supervised, forked-worker and in-process backends, each carrying the plan's environment. The forked worker's `WorkerEvent::CloneProgress` events (`SpawnClient::clone_repo_observed`) and the in-process `clone_run::clone_with_progress` reach `on_progress`. `on_started` receives the `CloneHandle` that stops the clone once git runs: from the supervised and in-process runners directly, and as `CloneHandle::for_process_group(pid)` from the forked worker's `WorkerEvent::Started` |
| `clone_stopper.rs` | `CloneStopper::watching(cancel, runtime)`: watches the start's `CancellationToken` from the async runtime while the clone blocks a pool thread, and stops the clone through the handle `started(handle)` records — at once if the token was already cancelled when git began. Dropping it stops the watch. `ensure_project_available_for_start` answers `CANCELLED` (`start_task::refuse_a_cancelled_start`) for a clone that ended because it was stopped, not the error its killed git left |
| `start_keepalive.rs` | `with_activity_keepalive(sink, interval, activity, work)`: awaits `work` and emits `StartKeepAlive` naming `activity` every `START_KEEPALIVE_INTERVAL` (5 s) until it ends. A streamed start wraps its whole life in it (`STARTING_ACTIVITY`); `with_keepalive` is the `PROVISIONING_ACTIVITY` form around provisioning, so two keep-alives tick per interval while a project provisions |

## A streamed start is a task

`stream_start_session_at_session_coordinate` spawns a `StartTask` (`start_task.rs`, `TaskBody`) in
`LaunchSessions.task_registry` under kind `start_session` and returns the stream its events go to. The
task sends `started` (`StartTaskStarted { task_id, daemon_instance_id }`) first, then runs
`start_session_core_cancellable(req, sink, cancel)` (`harness_profile_request.rs`) with the task's
token inside the whole-start keep-alive, forwarding progress through its own task so the terminal
event cannot overtake a phase's end. `start_session_core` is that call with a token nothing signals,
which is what the unary `StartSession` and `EnsureProjectHomeSession` use.

The token reaches the start in three places:

- the clone, through `StartHandles { progress, cancel }` → `ProjectClone.cancel` → `CloneStopper`;
- `start_task::refuse_a_cancelled_start` after the clone in `start_session_on_request`, so a cancelled
  start cuts no worktree and spawns no agent;
- the split start (`SplitSessions::start_split_claude_cli_session`, in `tddy-session-split`), which
  races its forward against it.

When the start answers successfully but the token was cancelled meanwhile (a worktree cut or agent
spawn is not interrupted), `undo_a_cancelled_start` deletes the session it created through
`delete_session_at_session_coordinate` and the stream ends `Status::cancelled("start cancelled")`
(`START_CANCELLED`). If that delete fails, it ends `INTERNAL` naming the session. The task's status is
`Cancelled` for a `CANCELLED` end, `Failed` for any other error, `Completed` otherwise.

`LaunchSessions.askpass` is the daemon's one `AskpassEndpoint` (`None` on a host the daemon runtime did not
build, whose SSH clones are given no helper). Behaviour:
[cloning over SSH](../../../docs/ft/daemon/clone-ssh-passphrase.md); the endpoint:
[clone askpass](../../tddy-host-service/docs/clone-askpass.md).

## The CLI spawns

| Module | Holds |
|---|---|
| `claude_cli_spawn.rs`, `claude_cli_spawn_steps.rs` | what is left of the claude-cli spawn, shared by every CLI agent: `record_main_terminal` (a running `main` terminal recorded in its session directory, rewritten while it runs and once more at exit), `ClaudeCliHooks` / `claude_cli_hooks` (the hooks' `tddy-tools` path, daemon URL, the session OS user's agent-tool socket — always given, so hooks report over it on every host — and a per-session token), `record_hook_answer_wait` / `record_answer_wait_secs`, the re-exported notify wiring (`notify_env`) and `record_role_of_session_type_before_spawn`. No start or resume is built here: every CLI agent starts through `opencode_cli_spawn::start_cli_harness_session` |
| `claude_cli_tmux.rs` | the tmux main terminal every CLI agent shares: `TmuxLaunched` (the attached main and the pane pid the session records), `in_tmux_blocking` (tmux calls on the blocking pool), `stop_tmux_session_of_failed_start`, `agent_tmux_session_is_alive` (the bounded probe) and `reattach_recorded_main(manager, agents: Option<&HostCodingAgents>, data_dir, os_user, session_id)` — the lazy re-attach `tddy-session-lifecycle`'s `MainReattach` port calls (`TmuxMainReattach` passes the host's coding agents). It reads `.session.yaml`, re-attaches only a tmux-run session whose tmux session lives, resolves the agent from the recorded session type through `recorded_cli_session_type` and hands it to `cli_harness_tmux::reattach_recorded_harness`, then registers the main recording. A record without a session type is skipped with a warning (no default agent); a host without coding agents re-attaches nothing (logged as the `FAILED_PRECONDITION` a start gets); `false`, logged, otherwise |
| `cli_harness_tmux.rs` | every CLI agent **in tmux** (claude-cli, cursor-cli, opencode, muse, warp; `#tmux-agent` 3/3): `HarnessTmuxLaunch` (argv already built, pane env, `on_exit`), `start_harness_in_tmux` (`start_agent_tmux_session` with `remain_on_exit = on_exit.is_some()`, attach, then `CliSessionManager::run_exit_hook_on_agent_exit`), `resume_harness_in_tmux` (re-attach a running pane, re-register the hook; a dead or gone one is ended and started fresh), `reattach_recorded_harness` (the lazy re-attach for a recorded session: kind = session type, the hook re-registered — `recorded_exit_hook`, the resume-token recorder for a spec with a resume adapter and none for an agent resumed by its worktree, armed at once; a dead pane runs the hook and attaches nothing) and `settle_unwatched_agent_exit` (before a Resume reads the token: a dead pane's final screen through the hook, then the tmux session ended) |
| `cursor_cli_spawn.rs`, `chat.rs` | `chat` holds one re-export, `parse_created_chat_id` (the chat id `cursor-agent create-chat` printed); `cursor_cli_spawn` re-exports `chat`. Every cursor-cli start, resume and re-attach is the generic CLI launch, rendering `cursor.yaml`; Telegram's hand-built helpers went with `#agent-tools 18/19` |
| `operator_start.rs` | `LaunchSessions::start_session_for_os_user`: the generic start for an operator a satellite already identified by OS user — no token, no peer daemon — behind the `SessionStarter` port (`tddy-daemon-extensions`) |
| `cursor_from_spec.rs` | `node_launcher_of` (the bundled `node` and `index.js` beside a launcher-script binary, as `NodeLauncher`) |
| `spec_launch.rs` | the spec-driven host PTY launcher. `HostCodingAgents` (the agent registry plus the `ToolHost` it resolves against) and `resolve(id, binary_override)` produce a `HostAgent { spec, tool, deps }`. `plan_host_pty(agent, params, slots) -> HostPtyPlan { argv, env }` refuses a non-`pty` transport (`SpecLaunchError::TransportNotPty`), writes the slot files and renders `run.argv` with the slot flags spliced; `launch_host_pty(agent, params, slots, worktree, HostPtySession)` spawns that plan through `CliSessionManager::spawn_rendered_main`; a tmux-run start or resume starts `plan_host_pty`'s argv in tmux instead (`cli_harness_tmux`). The generic launch spawns through `launch_host_pty_without_inherited_tddy` (the argv under `/usr/bin/env -u` for each `TDDY_*` the daemon holds and the launch does not set) and, in tmux, `in_a_pane_without_inherited_tddy` (the pane unsets every inherited `TDDY_*` but the launch's own before it execs the agent). `AgentToolset` / `McpRegistration` carry a role's tool flags into the slots |
| `slot_file_over_ssh.rs` | `write_slot_file_over_ssh(alias, path, bytes)`: an ssh session's slot file written on the ssh target (`mkdir -p` + `cat >`). Every command on the target goes through `run_over_ssh(ssh, alias, path, remote, stdin, deadline)`, which runs the shell command `remote` with `stdin` on its standard input over `ssh -o BatchMode=yes -o ConnectTimeout=15 -o ServerAliveInterval=5 -o ServerAliveCountMax=3`, serves stdin, stdout and stderr off the waiting thread, and returns what the command printed (a stdout that cannot be read is the command's error). Bounded by `SLOT_FILE_OVER_SSH_DEADLINE` (60 s): past it the ssh is killed and the launch refused with `SpecLaunchError::SlotFileOverSshTimedOut { host, path, after }`; a non-zero exit is `SpecLaunchError::SlotFile` carrying the target's stderr |
| `host_coding_agents.rs` | `host_coding_agents(tddy_data_dir)`: `load_agent_registry(<tddyhome>/tools)` resolved against `daemon_tool_host(ProcessToolHost)` — the spec's `binary.env`, then `$HOME/.local/bin`, then `PATH` without wrapper shims, `tddy-tools` defaulting to the daemon binary's sibling, with no daemon.yaml input; a binary found nowhere is refused naming every place searched (`#agent-tools 19/19`). `no_coding_agents()` is the empty registry of a host that installed none. `tddy-daemon`'s `runtime::build` calls it once and fails the start on an error; the one `Arc` is handed to `DaemonSessionHost::new`, Telegram and the reserved-ids step, and reaches launches as `LaunchSessions::coding_agents` |
| `hooks_and_urls.rs` | the hook and URL helpers a spawn uses |
| `notify_env.rs`, `agent_notify_guidance.md` | what an agent is given so `tddy-tools notify` reaches its operator: `notify_env_pairs` (session, OS user, hook token, and the daemon socket **or** URL; a daemon-launched agent is always given its OS user's agent-tool socket), `AGENT_NOTIFY_GUIDANCE`, and `agent_notify_launch` / `resumed_agent_notify_launch` (`AgentNotifyLaunch` → `AgentNotifyWiring`: the pairs plus `<session dir>/agent-system-prompt.md`, the guidance composed with a managed recipe's prompt), which `cli_agent_steps` applies for a spec declaring `operator_notify`. The pairs are not exported to the agent: they go into the session pointer's `notify` map (`cli_session_pointer`), which a shell-run `tddy-tools notify` reads. A session with a standing role gets neither |
| `cli_session_pointer.rs` | the `.tddy/session.json` pointer (`tddy_agent_backend::session_pointer::SessionPointer`) the generic CLI launch writes into the session's worktree at every start and resume, so a `tddy-tools submit` / `transition` / `ask` / `notify` the agent runs from its own shell reaches the session with no `TDDY_*` variable in the agent process ([session resolution](../../tddy-tools/docs/session-resolution.md)). `session_pointer_of(tool_server_env, notify, worktree)` (the managed workflow's `TDDY_SOCKET` / `TDDY_SESSION_DIR` / `TDDY_REPO_DIR`, the `TDDY_REMOTE_*` pair and the notify pairs; `None` when there is neither socket nor notify). The pointer's `owner` is the daemon (`SessionPointerOwner::this_process()`). `PointerHost::{Local, Ssh { ssh, alias }}` is where the worktree is (`PointerHost::of(remote_host)`, the `ssh` on `PATH`; `through(ssh, …)` names the client). `place_session_pointer(pointer, worktree, host) -> Option<PlacedSessionPointer { path, host }>`: on this host owner-only `0600`, replacing a stale one, added to the repo's `info/exclude` when the worktree lies in a git checkout — a plain directory gets no exclude; on an ssh target **one** bounded `run_over_ssh` command does both — `set -e; umask 077`, the checkout's `git rev-parse --path-format=absolute --git-path info/exclude` gets `/.tddy/session.json` appended unless `grep -qxF` finds it (a target worktree that is no checkout gets none), then `mkdir -p` + `cat >` from stdin, every path single-quoted; a failure is `DEADLINE_EXCEEDED` past the deadline, else `INTERNAL` naming the host. `removing_session_pointer_on_exit` chains the removal ahead of the launch's `on_exit` (on a PTY always; in tmux only when the launch has an exit hook, since only a hook keeps the pane) and runs it on a blocking thread, since an ssh target's removal is a bounded `rm -f` over ssh; `remove_session_pointer(worktree, host)` is the same removal for `DeleteSession`, from the worktree and `ssh_config_host` the `.session.yaml` records. A removal that fails on the target is logged, not raised. `record_remote_owner(placed, pointer)` and `REMOTE_AGENT_PID_FILE` (`.tddy/agent.pid`) rewrite an ssh pointer with the remote agent as its owner — one bounded command reads and removes the pid file, reads `hostname`, validates both and rewrites the pointer `umask 077`; they have no caller, since no launch runs the agent on the ssh target ([todo](../../../docs/dev/todo/2026-11-07-ssh-agent-launch-remote-pointer-owner.md)), so an ssh pointer is owned by the daemon, which a `tddy-tools` on the target does not check. A jailed session writes none: its shell relay carries the session env. Unit tests in `cli_session_pointer/cli_session_pointer_unit_tests.rs` (the ssh ones through a stub `ssh` that records the remote command and runs it locally) |
| `split_session_pointers.rs` | `HostSplitSessionPointers`, the `tddy-session-split::split_ports::SplitSessionPointers` port on this host: `place` writes a split agent's pointer into its context dir with `cli_session_pointer`'s helpers (always the local host — a split agent runs where its daemon does) and returns the exit hook that removes it; `remove` is for a spawn that failed after placing. The generic resume places a split session's own `SplitAgentWiring.session_pointer` instead of the wiring's, and refuses a split session without one |

## The sandboxed starts and relaunch

Every sandboxed CLI start and its relaunch — claude-cli, cursor-cli, opencode-cli, muse-cli — run
through the **shared jail launcher**, by one generic path (`svc_start_sandboxed_cli_harness_session`,
`svc_resume_sandboxed_cli_harness_session`, `cli_harness_jail`): the agent is launched from its tool
definition alone, and no module on this path names an agent.

| Module | Holds |
|---|---|
| `jail_launch.rs` | `LaunchSessions::launch_jailed(JailSpec, &TemplateParams, &RenderedSlots, JailedSession, role) -> Result<u32, Status>`. `JailSpec { agent: &HostAgent, defs: &[ToolDef], host: &dyn ToolHost }` is the agent's resolved closure and the definitions it came from; `JailedSession` is the session's jail, directories, host-side `session_env`, the daemon-added `env`, the managed workflow, `managed_codebase` and the provider route. In order: `record_role_of_session_type_before_spawn` writes the role into the session's metadata; `tddy_daemon_sandbox::tool_jail::lower_tool_jail` lowers the closure's `home`, `files`, `secrets`, `policy` and `network` (the route's base-URL host admitted); the agent's argv is rendered with the slot flags spliced and written with its env and slot files as the runner's `ExecPlan` at `<session>/sandbox/exec-plan.json` (`EXEC_PLAN_FILE`); `tddy-sandbox-runner --exec-plan` is spawned on `build_tool_jail_plan`'s plan; then ready (`JAIL_READY_TIMEOUT`, 120 s), `dial_and_bridge_with_egress` (the host relay held to the jail's `EgressGuard`) and registration in the sandbox manager. The plan's env is the spec's `run.env`, the daemon's non-`TDDY_*` additions, the definition's overlays and `HOME`, and `PATH` = the jail's tool `bin/` ahead of the jail baseline. `jail_home` resolves the definition's persistent home before lowering, so a placed `{home}` slot renders against it. `mcp_server_env` is the `env` of the `tddy-tools --mcp` server a jailed agent registers: `TDDY_SANDBOX_TOOL_IPC`, `TDDY_SANDBOX_SESSION_ID`, `TDDY_TOOLS_LOG_FILE`, `TDDY_TOOLS_ACCOUNTING_FILE`, `RUST_LOG` and the daemon's `TDDY_*` additions — the agent process itself sees no `TDDY_*` variable |
| `svc_start_sandboxed_claude_cli_session.rs` | the parameter structs every jailed start shares: `JailSession<'a>`, `JailDirs`, `type ManagedJailEnv` |
| `jail_launch_steps.rs` | `jail_relay_identity`, `managed_jail_env` and `record_jailed_main` — every jailed start and relaunch (`launch_jailed` included) registers the session's `main` recording right after inserting its sandbox state |
| `jail_session_files.rs` | `prepare_jail_dirs`, `prepare_jail_context_dir`, `write_jail_session_metadata` (records `session_role`) |
| `svc_relaunch_sandboxed_runner.rs` | `type RelaunchManagedEnv`, which the sandboxed resume uses, and the re-export of `relaunch_jail_dirs` and `relaunch_jail_steps` |
| `relaunch_jail_dirs.rs`, `relaunch_jail_steps.rs`, `jail_relaunch.rs` | `prepare_relaunch_dirs`, `refresh_relaunch_context_dir`; `relaunch_managed_workflow` (the one step `relaunch_jail_steps.rs` keeps); the jail relauncher's `rebuild` |
| `svc_start_sandboxed_cli_harness_session.rs` | `start_sandboxed_cli_harness_from_request` (provider account and `route.required_in_jail` first) and `start_sandboxed_cli_harness_session` for every jailed CLI agent: the pre-launch sequence (worktree on `cli_branch_intent`, the agent's hooks written into the worktree reporting over the in-jail tool-IPC socket when its spec declares `hooks` — `write_jail_hooks`), then `launch_jailed_cli_agent` → `launch_jailed` with `JailSession` / `JailDirs`. A first prompt goes through the spec's argv; a blank model is not refused. `warp-cli` never reaches it (`require_spec_placement`) |
| `svc_resume_sandboxed_cli_harness_session.rs` | `resume_sandboxed_cli_harness`, `relaunch_sandboxed_cli_harness` — the jail relaunched with `--resume` on the re-resolved route |
| `cli_harness_jail.rs` | what a jailed CLI agent launches from, read from its spec: `CliJail`, `JailWiring`, `launch_jailed_cli_agent` (its slots rendered for jail paths — the `tddy-tools` MCP registration, the roster's allow- and deny-lists and permission prompt for a spec taking them, the managed workflow's prompt as its append-system-prompt flag or rules file), `require_jail_route` (`route.required_in_jail`), `jail_route_config_env` (`route.config_env`), and the agent's argv rendered from `run:`; the daemon's `TDDY_*` additions go only to the `tddy-tools --mcp` server env. The MCP slot is rendered without its strict flag (`jailed`), so the agent also loads the project's `.mcp.json` ([todo](../../../docs/dev/todo/2026-11-02-jailed-claude-loads-project-mcp-json.md)), and `prompt_ahead_of_slots` puts the first prompt ahead of the then-variadic MCP flag. The legacy runner-hosted jail below the old `--agent-kind` line (`sandboxed_agent_kind`, `require_sandbox_hosts`, `sandboxed_runner_model`, `refuse_first_prompt`, `require_provider_route`, `resolve_cli_jail_binaries`, `cli_harness_runner_argv`) was deleted by the `#agent-tools` sweep (#828); `canonical_exec_path` is still used by the jail launcher |
| `jail_env_builders.rs` | `specialized_subagent_env`, `jail_daemon_identity_env`, `lsp_tools_env`, which every sandboxed path uses |

Coverage: `tddy-session-lifecycle`'s `sandboxed_claude_launch_baseline` and
`claude_cli_jail_from_spec_acceptance` (macOS Seatbelt), see
[test suites](../../tddy-session-lifecycle/docs/test-suites.md).

## GitHub identity of a session

`session_acting_identity.rs` resolves what a session acts as, from one of two sources, into a
`SessionIdentity { git_environment, github_credential, local_gh_hostname, is_local_gh }`
(behaviour: [session identity](../../tddy-session-lifecycle/docs/session-identity.md)).

- `ProjectGithub::{Vault(&[AccountAssignment]), LocalGh { os_user, git_url }}` names the source.
  `SessionAccountAccess::project_session_identity` resolves it; `session_identity_in` picks the source
  (the session's recorded `github-source.yaml`, else the row's `local_gh`) and resolves it.
- `SessionAccountAccess::with_github_cli(GithubCli)` injects the `gh` the Local gh resolution asks;
  the default is `GithubCli::on_spawn_child_path()`.
- A Local gh identity carries the four commit pairs of the `gh` login plus `TDDY_GITHUB_SOURCE`, and a
  `github_credential` that only refuses. When `gh` cannot answer the pairs are absent and the source
  entry is still present; when the git URL names no host, neither is.
- `is_local_gh` is set independently of the hostname, so the relay guard cannot be bypassed by a
  project whose git URL names no host.
- `SessionIdentity::relayed_by(RelayUser, logged_out_gh_config)` is what the jail launch and relaunch
  (`jail_launch_steps.rs`) apply to the host-side relay: the identity unchanged when the relay runs as
  the session's OS user (`RelayUser::SessionUser`, compared by uid) and for any vault identity;
  otherwise, for a Local gh identity, only `GH_CONFIG_DIR=<empty dir under the data dir>`, so `gh` in
  the relay's commands is logged out instead of acting as another user's login.
- `local_gh_github_token(&GithubCli, os_user, git_url)` is what daemon-side operations use
  (`RepointPlannedPr`): the host's `gh` token as the session's OS user, refused in `gh`'s or the
  resolver's words.

`start_request_checks.rs` holds the forwarding half: `forwarded_start_request(req, local_row)` fills an
unset `StartSessionRequest.github_source` from this host's row before a start is forwarded to a peer
(a request that already names one keeps it; no row names none), and `record_requested_github_source`
writes the source a start named into the new session's directory (`github-source.yaml`, owned by
`tddy-changeset`). A start that named none records nothing.

## Provider route of a session

`session_provider_route.rs` applies a [provider route](../../tddy-agent-backend/docs/architecture.md#provider-routes-provider_route)
to a claude-cli session (behaviour:
[Claude Code CLI session § Running on a provider account](../../../docs/ft/daemon/claude-cli-session.md#running-on-a-provider-account)).

- `SessionAccountAccess::provider_account(account_id)` (in `session_acting_identity.rs`) reads the
  model-provider account from the session owner's vault through the same `held_accounts` read the
  GitHub identity uses, with the same `SessionIdentityRefusal`s. The account is matched by id, then by a
  unique label (`provider_account_lookup.rs`); nothing matching is `Vault(NotFound)`, a shared label
  `AmbiguousLabel`.
- `session_route_sources(&StartSessionRequest)` maps `provider_account_id` / `provider_built_in` to
  `RouteSources.session` (the account wins when both are sent).
- `session_route_sources_with_default(req, harness, access)` adds `RouteSources.user_default` from
  `SessionAccountAccess::default_provider_account(harness)` — read only when the request names no
  account and no "built in". `default_provider_account` is `Ok(None)` without vaults or with an
  uninitialized vault, and any other vault refusal (locked, no such session) is returned and mapped
  like `provider_account`'s. Every CLI start uses it — the unsandboxed one in `cli_branch_starts.rs`
  and the sandboxed one in `svc_start_sandboxed_cli_harness_session.rs`.
- `resolve_session_route(access, harness, sources, model)` applies `choose_route` and, for an
  account (`route_on_account`), first refuses a harness that cannot be routed at all
  (`harness_routable` — Cursor, Warp — named in the refusal), then an empty model
  (`INVALID_ARGUMENT`), then reads the account and resolves the route.
  Vault refusals map to `FAILED_PRECONDITION` (no vaults, locked, uninitialized), `UNAUTHENTICATED`
  (no such session), `NOT_FOUND`, `INTERNAL` (vault unavailable); `RouteRefusal::NotYetRoutable` is
  `UNIMPLEMENTED` and every other refusal `INVALID_ARGUMENT`.
- `start_route(...)` → `Option<StartRoute { env, account }>`: a start's route resolved before the
  session directory exists (`refuse_a_claude_cli_start_up_front` checks with it); `StartRoute::record_in`
  writes `provider-account.yaml` once the directory is created. The generic CLI launch resolves and
  records its route through `cli_route_launch::{start_cli_route, resume_cli_route}`.
- `pty_route_env(&HarnessRoute)` flattens `env` and `secret_env` into PTY pairs. The route's `args`
  are applied by the caller (the generic launch appends them to the argv).
- `route_sources_on_resume(session_dir)` / `resume_route_env(access, harness, session_dir)` re-read
  the record and re-resolve it against the vault.
- `sandboxed_start_route(access, harness, sources, model, session_dir)` is the sandboxed starts'
  route: `None` when nothing was chosen, else the account's route, resolved before anything is cut
  and recorded in `provider-account.yaml` (never the key). `resume_route(access, harness, session_dir)`
  re-resolves that record for a jail relaunch; `resume_route_env` flattens it for a PTY. The jail
  receives the route through `SandboxRunnerSpawn.route`.
- `cli_route_launch.rs` applies a route to any [CLI harness](../../tddy-agent-backend/docs/architecture.md#cli-harnesses-cli_harness):
  `cli_route_launch(spec, route, session_dir) -> CliRouteLaunch { env, args }` writes the route's
  files `0600` (`write_atomic_with_mode`) into the session directory and points the spec's
  `route.config_env` (`OPENCODE_CONFIG`) at the first; `None` adds and writes nothing.
  `start_cli_route(access, agent, spec, sources, model, session_dir)` (`agent` the spec's `RoutedAgent`) resolves the chosen account
  (`route_on_account`), writes the files and records `provider-account.yaml`; `resume_cli_route`
  re-resolves the record and writes the files again. opencode-cli and muse-cli use them.
  `launch_model_for(spec, routed, model)` is the model the launch names itself: blank for a routed
  spec with a `route.config_env` (OpenCode — the route's `--model tddy-route/<model>` names it), the
  session's own otherwise (Muse Code's route is the key alone).
- `tool_session_route.rs` routes a **tool session** (headless `tddy-coder`):
  `tool_session_route_json(access, agent, sources, model) -> Result<Option<String>, Status>` returns
  `None` when nothing is chosen (the vault is never read), refuses `INVALID_ARGUMENT` for an empty
  agent and for an agent that is not a harness (`stub`, a specialized definition), and otherwise
  resolves through `resolve_session_route` (Cursor refused there) into
  [route-file JSON](../../tddy-agent-backend/docs/architecture.md#routed-backends-provider_route_file-backendrouted).
  `spawn_tool_session` resolves it before anything is created and sets
  `ToolSpawnPlan.provider_route_json`; `tddy-spawn` writes it to `<session_dir>/provider-route.json`
  (`0600`, chowned to the session user) and passes `--provider-route-path`. Resume calls
  `resumed_tool_session_route_json`, which re-resolves the start's `provider-account.yaml` record.
- `StackChildSpawnHandler` and `GrillMeConversationSpawnHandler` pass `RouteSources::default()`:
  a spawned child or conversation runs on its vendor.

Tests: `src/session_provider_route_tests.rs` — a real unlocked vault holding a Meta account gives the
PTY env and records the account and model but never the key; a start choosing nothing, a locked
vault, an unknown account, Cursor, an empty model and a sandboxed start are each refused or left on
the vendor as above; the request mapping; and resume with a record, with none, and after the account
was removed. `src/tool_session_route_tests.rs` — a Codex tool session on Meta gets a route file with
the Responses provider and the key; Cursor, a specialized definition and an empty agent are refused;
choosing nothing gives no file; every Responses preset in `tddy_accounts::presets()` is declared to
Codex under its own label. `src/default_route_tests.rs` — over a real vault whose Meta account is the
claude-cli default: a start naming nothing reads it and runs on Meta; an explicit built-in choice
overrides it and needs no unlocked vault; a locked vault refuses a start naming nothing; another
harness has no default.

## Placement and the session's git environment

`placement.rs` classifies where a start's codebase goes: `CodebasePlacement` (co-located, split, or
sandboxed), `PlacementRequest` (everything a start says about it, gathered into one value) and
`classify_placement`, which refuses any request asking for more than one placement and names both it
found. `classify_start_placement` applies it to a `StartSessionRequest` (refusal as `INVALID_ARGUMENT`)
for `svc_start_session_core.rs`, and `managed_recipe_for` resolves the workflow recipe a managed
codebase runs.

`session_git.rs` builds the git environment a session's own commits are made under:
`session_git_environment(&ActingIdentity)` (and `session_git_environment_of(&GitIdentity)`) returns the four
`GIT_AUTHOR_*` / `GIT_COMMITTER_*` variables, from the identity and nothing else, so the identity and the token that pushes it come from one value.
It is not the identity of a work-in-progress snapshot, which the daemon signs as itself.
`session_acting_identity.rs` is its caller; the suite is `tests/session_git_identity.rs`.

Neither module names LiveKit, and `tddy-daemon-livekit` declares neither.

## Session RPC handlers

`session_coordinate_handlers.rs` holds list, start, connect, the worktree snapshot and the streamed start,
as `impl LaunchSessions`. The list builds each entry with the free function `session_entry_from_listing`.
Its siblings `svc_resume_session.rs` (`resume_session_at_session_coordinate`) and `svc_signal_delete_session.rs`
are also `impl LaunchSessions`. `DeleteSession` kills a tmux-run session's tmux session first (`kill_recorded_tmux_session`, as the
session's OS user): a recorded one that cannot be killed refuses the delete with the session intact; with an
unreadable `.session.yaml` it kills `tddy-<session id>` anyway and only logs a tmux that cannot be run.
It then removes the session pointer from the worktree the `.session.yaml` records, on the ssh target it
records when it records one (`remove_recorded_session_pointer`, on a blocking thread), so a worktree kept
after the delete does not point a shell-run `tddy-tools` at a session that is gone.

### `ConnectSession` and the session-reach port

`ConnectSession` resolves the session, then reports a **reach**: for a session whose type this daemon
facilitates and which records a checkout, `tell_the_reach` calls
[`SessionReach::session_reached`](../../tddy-daemon-extensions/docs/extension-ports.md) with the
session and the transport the request arrived on (`request.metadata().transport()`, stamped by the
receiving host). The port decides what that costs the caller, and its error fails the request, so
`ConnectSession` itself opens no room and awaits LiveKit for no caller: a host with nothing wired
holds `NoSessionReach`, and with the LiveKit plugin wired only a caller that arrived over LiveKit
waits. The reply is dial-in metadata: empty LiveKit coordinates for every CLI harness session
(`claude-cli`, `cursor-cli`, `opencode-cli`) and `workspace` sessions (the room those coordinates would name is the session's *terminal* room, which
those types do not have), the session's terminal room otherwise.

"Facilitated here, over a checkout this daemon holds" is one rule, `facilitated_checkout(metadata)`,
answering `NotFacilitated`, `NoCheckout` or `Checkout(path)`. `ConnectSession` and an integration's
own RPC (through `ReachableSessions::resolve`) both apply it, so the two cannot disagree about which
sessions get a room. Its type half is `session_type_is_facilitated_here(session_type)` — every CLI
harness session type the registry holds (`claude-cli`, `cursor-cli`, `opencode-cli`) runs an agent
here, so it has a session room of its own; a `workspace` or `tool`
session does not. It lives here rather than in the LiveKit plugin because it is a statement about
session types, not about any transport.

This crate names no LiveKit crate. It reaches the plugin only through ports: `LaunchSessions` holds
`Arc<dyn SessionRooms>` and `Arc<dyn RoomAdmission>`, routes with
`tddy_host_service::peer_routing::PeerRouting` and `tddy_daemon_kernel::peer_forwarding::classify_peer_route`,
forwards a start with `tddy_daemon_kernel::peer_calls`, and measures a checkout for
`GetWorktreeSnapshot` with `tddy_worktree_service::worktree_measurement::snapshot_worktree_within`.

## Stacked, child and conversation spawns

| Module | Holds |
|---|---|
| `stack_parent.rs`, `stack_seed_validation.rs` | `StackParentHost` on the handle; `validate_stack_seed_base_session`, `session_repo_is_in_project` (public) |
| `stack_child_spawn.rs`, `child_spawn_handler.rs` | the `StackChildSpawnHandler` struct, and its `impl` |
| `conversation_spawn.rs`, `conversation_spawn_handler.rs` | `recipe_enables_conversation_spawn`, `session_tool_role` (a session's project-home tool entitlement: project-home, conversation spawner, ordinary), `conversation_branch_slug`, `GrillMeConversationSpawnHandler`, and its `impl` (the universal start: `start_conversation` / `conversation_start_status` / `cancel_conversation_start` over the orchestrator's start ledger) |
| `conversation_start.rs` | `LaunchSessions::begin_recorded_start` (refuse-before-begin, the streamed start, recorded in a start ledger) and `cancel_recorded_start` (local task or the peer's `CancelTask`), and `StartedSessionListings` — shared by the project-home `StartSession` tools and the conversation-spawn handler |
| `svc_pr_status_for_caller.rs` | the managed-workflow helpers and the stack-link half (`resolve_chain_base_ref_status`, `link_stack_node_to_spawned_branch`, `record_spawn_on_stack_node`) |
| `managed_launch.rs`, `worktree_source.rs` | `ManagedLaunch`, `prepare_managed_workflow_inner`; `WorktreeSource` |
| `conversation_worktree_op.rs`, `family_proto_bridge.rs` | the conversation-worktree operation a jail relays; the family proto bridge |
| `session_acting_identity.rs` | `SessionAccountAccess::session_identity`, `SessionIdentity`, `SessionGithubCredential` (pinned to the start's outcome) and `project_github_token`; see [session identity](../../tddy-session-lifecycle/docs/session-identity.md) |
| `host_session_socket.rs`, `inherited_host_sockets.rs` | `HostSessionSockets`: the per-OS-user host-session sockets, and the adoption of those `tddy-supervisor` hands the daemon (`host-session.<user>`). The owner-only bind and the descriptor vetting are `tddy_daemon_kernel::owner_socket` (`bind_owner_only`, `descriptors_named`, `adopt_descriptor`, with `OwnerSocketKind::HOST_SESSION`), shared with the agent-tool socket; these two files keep the registry, the session probe and the `host-session.` reading |
| (agent-tool socket) | not here: `LaunchSessions.agent_tool_sockets` is `tddy_daemon_kernel::agent_tool_sockets::AgentToolSockets`. `LaunchSessions::agent_tool_socket_of(os_user)` (`cli_agent_steps.rs`) resolves the session user's socket with `ensure_bound` **before anything is cut**, mapping a refusal to `FAILED_PRECONDITION`; every CLI start and resume (`cli_branch_starts.rs`, `opencode_cli_spawn.rs` via `CliSessionStart.agent_tool_socket`, `resumed_session_wiring`) passes the path down to the hooks, the notify pairs and the project-home main's `TDDY_REMOTE_DAEMON_SOCKET` (`project_home_launch.rs`). `hooks_report_over_the_agent_tool_socket(spec)` probes the spec's adapter alone |
| `session_worktree_observer.rs` | the `SessionWorktreeObserver` port and `announce_worktree_ready` |

## Edges

`tddy-agent-launch` depends on `tddy-session-split`, `tddy-cli-sessions`, `tddy-session-agents`,
`tddy-session-files`, `tddy-session-activity`, `tddy-daemon-livekit` and `tddy-daemon-kernel` among the
receivers, and on none of `tddy-session-lifecycle`. Nothing in `tddy-session-split`,
`tddy-session-agents`, `tddy-session-files` or `tddy-cli-sessions` depends on it.
