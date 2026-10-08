# `tddy-agent-launch`: module layout

The agent launch: how a session starts, resumes and relaunches, and how a started session spawns children.
The crate holds the topic code of what the daemon runs as `session.SessionService`'s start, resume, list,
connect, signal and delete handlers; `tddy-session-lifecycle` wires it to the host
([module-layout](../../tddy-session-lifecycle/docs/module-layout.md)) and re-exports it at the old
`tddy_session_lifecycle::…` paths.

About **9.2k production lines** in 52 modules, under the 10k receiver budget. One file is over 500:
`cursor_cli_spawn.rs` (563), moved whole, with its split deferred
([`docs/dev/todo/2026-10-08-cursor-cli-spawn-rs-in-tddy-agent-launch-is-563-production-lines.md`](../../../docs/dev/todo/2026-10-08-cursor-cli-spawn-rs-in-tddy-agent-launch-is-563-production-lines.md)).
Standing findings are in [`code-issues/`](code-issues/). A production line is any line of a `src/` file
outside an inline `#[cfg(test)] mod x { … }` block, excluding `*_tests.rs`.

## Launch sessions

`launch_ports.rs` defines `LaunchSessions`, an owned handle over the launch fields of the daemon host
(`config`, `tddy_data_dir`, `staging_base_dir`, `peer_routing`, `claude_cli_manager`, `sandbox_manager`,
`task_registry`, `session_stdio`, `agent_activity_hub`, `user_resolver`, `spawn_client`,
`workspace_sandboxes`, `rpc_activity`, `session_agent_inference`, `session_rooms`, `hosted_agent_clones`,
`session_admissions`, `worktree_observer`, `host_session_sockets`, and the agent topic's `AgentRoster` and
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
| `start_request_checks.rs` | `forward_start_session` (a start owned by a peer), `provision_project_for_start`, `validate_stack_seed_against_project` |
| `workspace_branch_start.rs` | `seed_and_start_workspace_session` |
| `cli_branch_starts.rs` | the claude-cli and cursor-cli branches: `cli_start_prelude` (sessions base, new id, attachments and initial prompt, returned as a `CliStart`) |
| `tool_session_spawn.rs` | `spawn_tool_session` and `spawn_tddy_coder`: the one `tddy-coder` spawn, for both a start and a resume |
| `tool_spawn_plan.rs` | `ToolSpawnPlan` (what the child is spawned with) and `ToolSpawnPurpose` (`Start` / `Resume`) |
| `svc_ensure_project_available_for_start.rs`, `svc_index_workspace_worktree.rs` | project provisioning for a start; the semantic index over a workspace worktree |
| `svc_start_claude_cli_session.rs` | the non-sandboxed Claude CLI start; holds `SessionStdioEndpoint` (the reverse stdio endpoint to a spawned `tddy-coder`) |
| `svc_resume_claude_cli_session.rs`, `svc_resume_sandboxed_claude_cli_session.rs` | the resume paths; the sandboxed one relaunches through `relaunch_sandboxed_runner` |

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
| `jail_env_builders.rs` | `specialized_subagent_env`, `jail_daemon_identity_env`, `lsp_tools_env`, which all three paths use |

The Claude and relaunch paths are cut into matching steps; the three paths remain separate copies of one
launch sequence, and merging them waits on test coverage
([`docs/dev/todo/2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md`](../../../docs/dev/todo/2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md)).

## Session RPC handlers

`session_coordinate_handlers.rs` holds list, start, connect, the worktree snapshot and the streamed start,
as `impl LaunchSessions`. The list builds each entry with the free function `session_entry_from_listing`.
Its siblings `svc_resume_session.rs` (`resume_session_at_session_coordinate`) and `svc_signal_delete_session.rs`
are also `impl LaunchSessions`.

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
