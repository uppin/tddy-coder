# `tddy-agent-launch`: module layout

The agent launch: how a session starts, resumes and relaunches, and how a started session spawns children.
The crate holds the topic code of what the daemon runs as `session.SessionService`'s start, resume, list,
connect, signal and delete handlers; `tddy-session-lifecycle` wires it to the host
([module-layout](../../tddy-session-lifecycle/docs/module-layout.md)) and re-exports it at the old
`tddy_session_lifecycle::…` paths.

About **9.4k production lines** in 54 modules, under the 10k receiver budget. No production file is over 500 (the largest, by the count to the first `#[cfg(test)]`:
`svc_start_session_core.rs` 496, `svc_start_sandboxed_claude_cli_session.rs` 493,
`session_coordinate_handlers.rs` 485).
Standing findings are in [`code-issues/`](code-issues/). A production line is any line of a `src/` file
outside an inline `#[cfg(test)] mod x { … }` block, excluding `*_tests.rs`.

## Launch sessions

`launch_ports.rs` defines `LaunchSessions`, an owned handle over the launch fields of the daemon host
(`config`, `tddy_data_dir`, `staging_base_dir`, `peer_routing`, `claude_cli_manager`, `sandbox_manager`,
`task_registry`, `session_stdio`, `agent_activity_hub`, `user_resolver`, `spawn_client`,
`workspace_sandboxes`, `rpc_activity`, `session_agent_inference`, `session_rooms`, `hosted_agent_clones`,
`session_admissions`, `worktree_observer`, `session_reach`, `reachable_sessions`, `host_session_sockets`, and the agent topic's `AgentRoster` and
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
`StreamStartSession`: it checks the request, routes it, and dispatches by placement and session type. Its
steps are in sibling modules:

| Module | Holds |
|---|---|
| `harness_profile_request.rs` | `LaunchSessions::start_session_core` is a wrapper: it applies the [harness profile](../../../docs/ft/daemon/harness-profiles.md) named by `req.agent` (looked up in `<tddy_data_dir>/agents`) before the allowed-agents check and peer forwarding, then calls `start_session_on_request` (`pub(crate)`, in `svc_start_session_core.rs`). `start_session_core` refuses a requested role with `PERMISSION_DENIED` (`refuse_a_requested_role`) and applies the profile; `start_session_with_role` (`pub(crate)`) takes a request whose profile is already applied — `EnsureProjectHomeSession` is its one caller, and applies the profile itself (`apply_harness_profile`, `pub(crate)`) so it can check the session type the profile resolves to. `request_on_profile` puts the profile's account at the `agent_definition` level beside the request's own session-level choice, runs `choose_route` once, resolves the winner in the starter's vault and writes its **id** into `provider_account_id`; the profile's model is used only when the request gives none. Refusals name the profile and the account |
| `harness_profile_start.rs` | `profile_start(&HarnessProfile) -> ProfileStart { session_type, agent, choice: RouteChoice::Account, model }`: a CLI harness (`claude-cli`, `opencode-cli`, `muse-cli`, `warp-cli`, `cursor-cli`) becomes the session type, a tool-session harness (`claude`, `claude-acp`, `codex`, `codex-acp`, `opencode`, `cursor`) the agent; any other is `INVALID_ARGUMENT` |
| `provider_account_lookup.rs` | `find_provider_account`, behind `SessionAccountAccess::provider_account`: an id first, then a unique label; none is `NOT_FOUND`, a label on two accounts `SessionIdentityRefusal::AmbiguousLabel` (`INVALID_ARGUMENT`), never a guess |
| `start_request_checks.rs` | `refuse_a_cli_start_without_a_model` (claude-cli and cursor-cli need a model; shared by `start_session_on_request` and the project-home up-front check), `refuse_an_unsupported_cli_harness_start(harness, req)` (every other registered harness: refused over a managed codebase, and, when sandboxed, through `cli_harness_jail::require_sandbox_hosts`; unit tests in `cli_harness_start_checks_unit_tests.rs`), `forward_start_session` (a start owned by a peer, waiting `start_forward_deadline(config)` — the split-start budget, not the 30 s forward deadline), `provision_project_for_start` (keeps a streamed start alive while it waits), `validate_stack_seed_against_project` |
| `workspace_branch_start.rs` | `seed_and_start_workspace_session` |
| `cli_branch_starts.rs` | the claude-cli and cursor-cli branches and the start of every other registered CLI harness: `cli_start_prelude` (sessions base, new id, attachments and initial prompt, returned as a `CliStart`); `start_cli_harness(harness, …)` (`refuse_an_unsupported_cli_harness_start` first: over a managed codebase, and, for a sandboxed start of a harness the sandbox runner does not host — `warp-cli` — `FAILED_PRECONDITION` "the sandbox runner does not host <type>" before any provider route is resolved; then the prelude; then the jailed start when `sandbox` is set and the PTY start otherwise); `sandboxed_claude_route` (the route a sandboxed claude-cli start runs on, the person's default included); `refuse_a_claude_cli_start_up_front` (the refusals an unsandboxed claude-cli start would answer before it creates anything — model, caller-chosen id, recipe via `managed_recipe_for`, provider route — for a caller that must stop something first) |
| `cli_session_worktree.rs` | what a daemon-cut CLI session's worktree needs, shared by cursor-cli and opencode-cli: `provision_cli_worktree(&CliWorktreeStart, …) -> CliWorktree { path, project }` (cut or adopt the worktree, push the new branch when asked) and `semantic_index_env` |
| `opencode_cli_spawn.rs` | a registered CLI harness cut a worktree of its own — the [OpenCode](../../../docs/ft/daemon/opencode-cli-session.md), [Muse Code](../../../docs/ft/daemon/muse-cli-session.md) and [Warp](../../../docs/ft/daemon/warp-cli-session.md) sessions: `start_cli_harness_session(CliHarnessStart)` (route first, then worktree on `BranchIntentPolicy::cli_harness(session_type)`, seeded clones, commit identity, spawn with the resume-token recorder as its exit hook, `.session.yaml`, then the recorder armed) and `LaunchSessions::resume_cli_harness` (first `settled_resume_token`, then the descriptor's resume argv with `CliLaunch.conversation_id` = the recorded `harness_resume_token`; route re-resolved; refused when the worktree is gone). The binary is `DaemonConfig::cli_harness_binary` for the descriptor. `start_session_core` and `resume_session_at_session_coordinate` reach it through a registry lookup (`cli_harness(session_type)`) after their claude-cli and cursor-cli branches, so a further harness needs no dispatch arm |
| `cli_resume_token.rs` | the resume token a CLI harness prints as it exits (Warp's `warp --resume <TOKEN>`): `resume_token_recorder(session_dir) -> (ExitHook, ResumeTokenArm)` — the hook `launch_cli_harness` runs once the process has exited and its output is complete, before the terminal is deregistered; it parses the capture with `cli_harness_warp::resume_token_from_output` and writes only `harness_resume_token`, only when a token was found. It waits for `ResumeTokenArm::arm`, which the start and the resume fire once they have written the whole metadata (so that write cannot clobber the token); a dropped arm records nothing. `settled_resume_token(cli_manager, session_id, session_dir)` waits on `CliSessionManager::exit_hook_settled` and then re-reads the token, so a resume right after an exit continues the conversation. It runs for every generic-path harness (opencode-cli, muse-cli, warp-cli); only Warp prints the line |
| `tool_session_spawn.rs` | `spawn_tool_session` and `spawn_tddy_coder`: the one `tddy-coder` spawn, for both a start and a resume |
| `tool_session_route.rs` | a tool session's provider route as route-file JSON: `tool_session_route_json`, `resumed_tool_session_route_json`; see [Provider route of a session](#provider-route-of-a-session) |
| `tool_spawn_plan.rs` | `ToolSpawnPlan` (what the child is spawned with, including `provider_route_json`) and `ToolSpawnPurpose` (`Start` / `Resume`) |
| `svc_ensure_project_available_for_start.rs`, `svc_index_workspace_worktree.rs` | project provisioning for a start; the semantic index over a workspace worktree |
| `svc_ensure_project_home.rs` | `EnsureProjectHomeSession`: forwards a request naming another daemon, then holds a process-wide lock **before** reading the project, so two set-ups cannot both find no main; `caller_home_scope` (OS user, sessions base, projects dir, registered project); `recorded_main` (the session `ProjectData.home_session_id` names — cleared when its directory is gone, `INTERNAL` when its record is unreadable). Without `start`, `reach_recorded_main`: none → `NOT_FOUND`, running (pid alive or terminal served) → unchanged, stopped → `fast_forward_home` + `resume_session_at_session_coordinate`. With `start`, `set_up_main`: `refuse_what_main_cannot_honour`, `apply_harness_profile`, `refuse_a_type_main_cannot_run`, `home_worktree_for` (`ensure_home_worktree`), `on_home_worktree`, `refuse_a_claude_cli_start_up_front` — every check before anything stops; then `stop_previous_main` (fast-forward, stop a served recorded main) and `start_and_record_main` (`start_session_with_role`, then `project_storage::set_project_home_session`; a main that cannot be recorded is stopped again). `EnsureOutcome`: `started` / `replaced` / `resumed` / `running` |
| `project_home_start_spec.rs` | the start request main runs from: `refuse_what_main_cannot_honour` (a requested `session_role` → `PERMISSION_DENIED`; `sandbox`, `sandboxed_codebase`, `codebase_daemon_instance_id`, `ssh_config_host`, `agent_clone`, `split_agent` → `INVALID_ARGUMENT`), `refuse_a_type_main_cannot_run` (anything but `claude-cli`, checked after the profile is applied → `FAILED_PRECONDITION`), `on_home_worktree` (sets project, home-worktree `repo_path`, `managed_codebase`, the `project-home` role; clears branch, stack and `daemon_instance_id` fields; keeps the rest of the spec) |
| `claude_cli_spawn_steps.rs` (role) | `record_role_before_spawn` writes a session's role into its initial `.session.yaml` before the agent is spawned (nothing for an ordinary session); `role_allowed_tools_args` gives a `project-home` claude-cli one `--allowedTools` per `tddy_tool_engine::read_only::project_home_allowed_tools()` entry, at spawn and again at resume from the record |
| `clone_planner.rs`, `clone_backend.rs`, `clone_ssh_env.rs`, `start_keepalive.rs` | how a clone reaches its remote and runs; see [Cloning a project](#cloning-a-project-for-a-start) |
| `svc_start_claude_cli_session.rs` | the non-sandboxed Claude CLI start; holds `SessionStdioEndpoint` (the reverse stdio endpoint to a spawned `tddy-coder`). A non-empty `repo_path` is run in directly (`claude_cli_spawn_steps::claude_cli_checkout` adopts it, canonicalized, and never removes it) instead of cutting a session worktree — how the project-home session runs on its home worktree |
| `svc_resume_claude_cli_session.rs`, `svc_resume_sandboxed_claude_cli_session.rs` | the resume paths; the sandboxed one relaunches through `relaunch_sandboxed_runner` |

## Cloning a project for a start

`ensure_project_available_for_start` clones the project's checkout when the host has none. How the clone
reaches its remote is decided per clone, on the blocking pool, where the URL being cloned is known:

| Module | Holds |
|---|---|
| `clone_planner.rs` | `ClonePlanner` and `PlannedClone`: a facilitator clone runs with only its transport shim; an SSH remote runs with the askpass environment when the host's OpenSSH is 8.4 or later and an agent is reachable; otherwise it runs without prompting (`BatchMode=yes`) and a failure says why. The grant a clone holds lives until it ends. A refusal before the clone starts (an unresolvable session) is kept and returned as its own `Status`. |
| `clone_ssh_env.rs` | `ClonePlan::{Facilitator, Ssh}`, `clone_ssh_env_for`, `agent_socket_for` and `CloneAgentError` (`NoAgentReachable` and `LookupFailed`, kept distinct), `is_ssh_url`, `resolve_askpass_path` (beside the daemon executable, then `PATH`) |
| `clone_backend.rs` | `CloneBackend::clone_repo`: the supervised, forked-worker and in-process backends, each carrying the plan's environment |
| `start_keepalive.rs` | `with_keepalive`: awaits provisioning and emits `StartKeepAlive` every `START_KEEPALIVE_INTERVAL` (5 s) until it ends |

`LaunchSessions.askpass` is the daemon's one `AskpassEndpoint` (`None` on a host the daemon runtime did not
build, whose SSH clones are given no helper). Behaviour:
[cloning over SSH](../../../docs/ft/daemon/clone-ssh-passphrase.md); the endpoint:
[clone askpass](../../tddy-host-service/docs/clone-askpass.md).

## The CLI spawns

| Module | Holds |
|---|---|
| `claude_cli_spawn.rs`, `claude_cli_spawn_steps.rs` | `spawn_claude_cli_session_inner`, the non-sandboxed Claude CLI spawn, and its steps (`ClaudeCliWorktreeCut`, `ManagedClaudeCliLaunch`, …) |
| `cursor_cli_spawn.rs`, `chat.rs`, `resume.rs` | `spawn_cursor_cli_session_inner` (a public module, a wrapper over `spawn_cursor_cli_session_reporting`, which reports the start's phases) with `CursorCliSessionRecord<'a>`; hooks, `parse_created_chat_id`, `mint_cursor_chat_id`; `resume_cursor_cli_session` |
| `hooks_and_urls.rs` | the hook and URL helpers a spawn uses |

## The sandboxed starts and relaunch

| Module | Holds |
|---|---|
| `svc_start_sandboxed_claude_cli_session.rs` | `start_sandboxed_claude_cli_session`, and the file-local parameter structs `JailSession<'a>`, `JailBranch<'a>`, `JailDirs`, `JailLaunch`, `JailRunnerEnv<'a>`, `type ManagedJailEnv` |
| `jail_launch_steps.rs` | `warm_up_jail_agents`, `managed_jail_env`, `jail_semantic_index_env`, `launch_jail` |
| `jail_session_files.rs` | `prepare_jail_dirs`, `prepare_jail_context_dir`, `write_jail_session_metadata` |
| `jail_worktree.rs` | `project_default_branch_ref`, `create_jail_project_worktree`, `link_jail_branch_to_stack_node` |
| `svc_relaunch_sandboxed_runner.rs` | `relaunch_sandboxed_runner`, with `RelaunchJailEnv`, `RelaunchedRunnerSpawn`, `RelaunchedJailBridge`, `type RelaunchManagedEnv` |
| `relaunch_jail_dirs.rs`, `relaunch_jail_steps.rs`, `jail_relaunch.rs` | `prepare_relaunch_dirs`, `refresh_relaunch_context_dir`; `relaunch_managed_workflow`, `resolve_relaunch_binaries`, `relaunch_jail_env`, `spawn_relaunched_runner`, `bridge_relaunched_jail` |
| `svc_start_sandboxed_cursor_cli_session.rs` | `start_sandboxed_cursor_cli_session`, in one function |
| `svc_start_sandboxed_cli_harness_session.rs` | `start_sandboxed_cli_harness_from_request` (first prompt and provider account refused first, then the route) and `start_sandboxed_cli_harness_session` for `opencode-cli` / `muse-cli`, with `bridge_and_register_cli_harness_jail`; `warp-cli` never reaches it (refused by `require_sandbox_hosts`) |
| `svc_resume_sandboxed_cli_harness_session.rs` | `resume_sandboxed_cli_harness`, `relaunch_sandboxed_cli_harness` — the jail relaunched with `--resume` on the re-resolved route |
| `cli_harness_jail.rs` | the pure parts both use: `sandboxed_agent_kind`, `require_sandbox_hosts` (the runner's agent kind for a harness, `FAILED_PRECONDITION` "the sandbox runner does not host <type>" for one it does not host), `sandboxed_runner_model`, `refuse_first_prompt`, `require_provider_route`, `resolve_jail_agent_binary`, `resolve_cli_jail_binaries`, `cli_harness_runner_argv`, `canonical_exec_path` (also used by the claude start and the relaunch) |
| `jail_env_builders.rs` | `specialized_subagent_env`, `jail_daemon_identity_env`, `lsp_tools_env`, which all three paths use |

The Claude and relaunch paths are cut into matching steps; the four start paths remain separate copies of one
launch sequence, and merging them waits on test coverage
([`docs/dev/todo/2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md`](../../../docs/dev/todo/2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md)).

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
  like `provider_account`'s. The unsandboxed claude-cli and opencode-cli starts in
  `cli_branch_starts.rs` use it, and so do the sandboxed claude-cli, opencode-cli and muse-cli starts.
- `resolve_session_route(access, harness, sources, model)` applies `choose_route` and, for an
  account (`route_on_account`), first refuses a harness that cannot be routed at all
  (`harness_routable` — Cursor, Warp — named in the refusal), then an empty model
  (`INVALID_ARGUMENT`), then reads the account and resolves the route.
  Vault refusals map to `FAILED_PRECONDITION` (no vaults, locked, uninitialized), `UNAUTHENTICATED`
  (no such session), `NOT_FOUND`, `INTERNAL` (vault unavailable); `RouteRefusal::NotYetRoutable` is
  `UNIMPLEMENTED` and every other refusal `INVALID_ARGUMENT`.
- `start_route(...)` → `Option<StartRoute { env, account }>`: called first in
  `spawn_claude_cli_session_inner`, before the session directory exists; `StartRoute::record_in`
  writes `provider-account.yaml` once the directory is created, and `env` is appended to the PTY's
  `env_extra` after the managed-launch environment.
- `pty_route_env(&HarnessRoute)` flattens `env` and `secret_env` into PTY pairs. The route's `args`
  are not applied: the claude-cli argv already carries `--model <model>` for the same model.
- `route_sources_on_resume(session_dir)` / `resume_route_env(access, harness, session_dir)` re-read
  the record and re-resolve it against the vault; `LaunchSessions::resumed_agent_env` in
  `svc_resume_claude_cli_session.rs` assembles the resumed agent's environment (commit identity
  unless split, split wiring, route env) in one place.
- `sandboxed_start_route(access, harness, sources, model, session_dir)` is the sandboxed starts'
  route: `None` when nothing was chosen, else the account's route, resolved before anything is cut
  and recorded in `provider-account.yaml` (never the key). `resume_route(access, harness, session_dir)`
  re-resolves that record for a jail relaunch; `resume_route_env` flattens it for a PTY. The jail
  receives the route through `SandboxRunnerSpawn.route`.
- `cli_route_launch.rs` applies a route to any [CLI harness](../../tddy-agent-backend/docs/architecture.md#cli-harnesses-cli_harness):
  `cli_route_launch(harness, route, session_dir) -> CliRouteLaunch { env, args }` writes the route's
  files `0600` (`write_atomic_with_mode`) into the session directory and points the harness's
  `route_config_env` (`OPENCODE_CONFIG`) at the first; `None` adds and writes nothing.
  `start_cli_route(access, harness, sources, model, session_dir)` resolves the chosen account
  (`route_on_account`), writes the files and records `provider-account.yaml`; `resume_cli_route`
  re-resolves the record and writes the files again. opencode-cli and muse-cli use them.
  `launch_model_for(harness, routed, model)` is the model the launch names itself: blank for a routed
  harness with a `route_config_env` (OpenCode — the route's `--model tddy-route/<model>` names it), the
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
found. `svc_start_session_core.rs` is its only caller.

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
are also `impl LaunchSessions`.

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
| `conversation_spawn.rs`, `conversation_spawn_handler.rs` | `recipe_enables_conversation_spawn`, `conversation_branch_slug`, `GrillMeConversationSpawnHandler`, and its `impl` |
| `svc_pr_status_for_caller.rs` | the managed-workflow helpers and the stack-link half (`resolve_chain_base_ref_status`, `link_stack_node_to_spawned_branch`, `record_spawn_on_stack_node`) |
| `managed_launch.rs`, `worktree_source.rs` | `ManagedLaunch`, `prepare_managed_workflow_inner`; `WorktreeSource` |
| `conversation_worktree_op.rs`, `family_proto_bridge.rs` | the conversation-worktree operation a jail relays; the family proto bridge |
| `session_acting_identity.rs` | `SessionAccountAccess::session_identity`, `SessionIdentity`, `SessionGithubCredential` (pinned to the start's outcome) and `project_github_token`; see [session identity](../../tddy-session-lifecycle/docs/session-identity.md) |
| `host_session_socket.rs`, `inherited_host_sockets.rs` | `HostSessionSockets`: the per-OS-user host-session sockets, and the adoption of those `tddy-supervisor` hands the daemon |
| `session_worktree_observer.rs` | the `SessionWorktreeObserver` port and `announce_worktree_ready` |

## Edges

`tddy-agent-launch` depends on `tddy-session-split`, `tddy-cli-sessions`, `tddy-session-agents`,
`tddy-session-files`, `tddy-session-activity`, `tddy-daemon-livekit` and `tddy-daemon-kernel` among the
receivers, and on none of `tddy-session-lifecycle`. Nothing in `tddy-session-split`,
`tddy-session-agents`, `tddy-session-files` or `tddy-cli-sessions` depends on it.
