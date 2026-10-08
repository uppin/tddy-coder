# `tddy-session-split`: module layout

Split and sandboxed-codebase sessions: an agent paired with a codebase on a sandboxed checkout, with its
LiveKit room, its context read from the codebase host, and its teardown. About **3.4k production lines**
in 17 modules, none over 500. Standing findings are in [`code-issues/`](code-issues/).

The topic runs over `SplitSessions` (`split_ports.rs`), an owned handle over the host's split fields
(`config`, `tddy_data_dir`, `staging_base_dir`, `peer_routing`, `session_rooms`, `workspace_sandboxes`,
`workspace_sandbox_provisioner`, `claude_cli_manager`, `session_tokens`), plus the agent topic's
`AgentRoster` handle and `host: Arc<dyn SplitHost>`. `SplitSessions` is `Clone` and built per call by
`DaemonSessionHost::split_sessions()` in `tddy-session-lifecycle`. `trait SplitHost: AgentHostCallbacks`
has `start_workspace_session`, `delete_session`, `session_files` and `session_agents`; the host
implements it once.

| Module | Holds |
|---|---|
| `split_ports.rs` | `SplitSessions`, `trait SplitHost`, and the two aliases of the service surfaces its callbacks return (`SplitSessionFiles`, `SplitSessionAgents`) |
| `svc_spawn_split_agent.rs` | `spawn_split_agent` (the agent half of a split session), cut into `join_split_livekit_room`, `split_agent_context_and_args` and `write_split_agent_metadata`, with `SplitAgentProcess<'a>` |
| `svc_paired_codebase_teardown.rs` | `delete_paired_codebase_session`: tearing down the paired codebase session through `SplitHost::delete_session` |
| `split_claude_cli_start.rs`, `split_start.rs` | `start_split_claude_cli_session`; `SplitStartFailure` and split-placement resolution |
| `svc_split_context_from_codebase_host.rs` | the split agent's context read from the codebase host |
| `svc_start_sandboxed_codebase_session.rs` | a workspace start combined with `spawn_split_agent`, through `SplitHost::start_workspace_session` |
| `svc_resume_split_wiring.rs` | the split half of a resume: `resume_split_wiring` and `split_roster_from_codebase_host` (reads the roster through `SplitHost::session_agents`) |
| `svc_provision_workspace_tool_sandbox.rs` | `provision_workspace_tool_sandbox` |
| `attached_initial_prompt.rs` | `attached_initial_prompt`, over `AttachmentState`; the split agent and the launch topic's CLI starts call it |
| `svc_resolve_tddy_tools_path.rs` | `resolve_tddy_tools_path` and `agent_tool_socket_for_embedded_host`, both on the handle |
| `service_util.rs` | the helpers several start, resume and spawn paths share |
| `workspace_session.rs` | `workspace_sandbox_spec`, which sandbox provisioning and the jail rebuild both call, and the workspace session's own `.session.yaml` reads |
| `split_session.rs`, `agent_argv.rs`, `agent_credentials.rs` | split-session start; native-tool constants, roster withdrawals and `split_claude_extra_args`; the token TTL, `mint_agent_session_token`, `verified_caller` and `RoomPollTokenMinter` |

## `service_util.rs`

Each of these is the one definition several start, resume and spawn paths call: `find_registered_project`
and `project_repo_root` (the project registered for an OS user, and its checkout, refused if missing);
`starting_session_metadata`; `write_initial_changeset`; `create_session_worktree` (cut under the spawn
deadline); `index_session_worktree` (blocks until the index is terminal; a missing embedder or failed index
is an error, with no unindexed fallback); `spawn_blocking_with_timeout` and `await_supervised_with_timeout`
(public, for the RPC handlers above the wiring crate); `push_new_branch_to_origin_if_requested`;
`resume_agent_and_recipe`; and `write_claude_hooks_settings` and `resolve_start_session_claude_binary`
(through `config::resolve_claude_binary_path`, so the interactive and sandboxed paths never pick
differently). The "trim, and treat empty as unset" conversion is `tddy_daemon_kernel::trim_to_option`.

## Edges

`tddy-session-split` depends on `tddy-cli-sessions`, `tddy-session-agents`, `tddy-session-files`,
`tddy-session-activity`, `tddy-daemon-kernel`, `tddy-daemon-livekit` and `tddy-daemon-sandbox` among the
receivers. It does not depend on `tddy-agent-launch` (that crate depends on it) or on
`tddy-session-lifecycle`.
