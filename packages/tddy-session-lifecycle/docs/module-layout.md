# Module layout

How `tddy-session-lifecycle`'s `src/` is organised: the topics the crate holds, the module tree
they live in, the helpers they share, and the facades over the modules that live in the crates below
it. The RPC surface is [session-service.md](session-service.md);
the test suites are [test-suites.md](test-suites.md).

## Sizes, and how they are counted

116 non-test `src/*.rs` files hold about **20,800 production lines**. One file is over 500
production lines, `cursor_cli_spawn.rs` (532); the next largest are
`connection_service/svc_start_sandboxed_claude_cli_session.rs` (495), `connection_service.rs` (488)
and `connection_service/svc_start_session_core.rs` (480). Six production functions are over 150
lines, each with a recorded reason: five in
[`docs/dev/todo/2026-09-24-lifecycle-functions-still-over-150-lines.md`](../../../docs/dev/todo/2026-09-24-lifecycle-functions-still-over-150-lines.md),
and `handle_rpc` in [`code-issues/`](code-issues/).

A **production line** is a line outside every `#[cfg(test)]` item (a `mod`, a `use`, an inline test
block), with test-only files excluded: those declared behind `#[cfg(test)] mod x;`, and
`*_tests.rs`, `tests.rs`, `test_util.rs`. This is the **inline-test-block rule**. Counting only to a
file's first `#[cfg(test)]` gets this crate wrong: `connection_service.rs` declares `#[cfg(test)]
use` lines near its top, so that count reads 18 for a file of 433 production lines. The script that
applies the rule is in the change history,
[`2026-09-23-carve-lifecycle-destructure`](../../../docs/dev/changesets/2026-09-23-carve-lifecycle-destructure.md#loc-assessment).

## `connection_service`: the RPC host

`connection_service.rs` (488 production lines) holds `DaemonSessionHost`, its `mod` declarations,
and the `pub use` lines that keep the crate's public paths stable. Everything else is a child
module. Each `svc_*` file is an `impl DaemonSessionHost` block for one step or family, so every child
reads the host's private fields directly and needs `pub(super)` at most. A child's own children use
`pub(super)` for their parent, and `pub(in crate::connection_service)` for an item another branch
of the host calls.

### Topic files cut out of the host

| File | Holds |
|---|---|
| `placement.rs` | `CodebasePlacement` and the `classify_*` functions: where a session's agent and its worktree go. `pub use`d, because `tddy-daemon-rpc`'s suites name them |
| `worktree_source.rs` | `WorktreeSource` |
| `split_start.rs` | `SplitStartFailure` and split-placement resolution; its child `split_start/split_claude_cli_start.rs` holds `start_split_claude_cli_session` |
| `peer_session_answer.rs` | the four free items that read or classify a peer's answer about a session: `peer_has_no_such_session` (a peer's `FailedPrecondition` or `NotFound`), `split_pairing` (the codebase daemon and session a split session is paired with, `None` for half a pairing), `resolve_worktree_root_for_session` and the free `resolve_exec_tool_worktree` (not the host method of the same name). A `pub(crate)` module; `workspace_session` and `connection_service` re-export the two that something outside the crate names (`resolve_worktree_root_for_session`, `resolve_exec_tool_worktree`), and the other two have no facade |
| `seeded_clone_guard.rs` | `SeededAgent`, `SeededCloneGuard` and the release it carries. `SessionStdioEndpoint` (the reverse stdio endpoint to a spawned `tddy-coder`) is in `svc_start_claude_cli_session.rs`, and `ExecToolRoute` (where one exec tool call runs) beside `LocalExecTools` in `local_exec_tools.rs` |
| `managed_launch.rs` | `ManagedLaunch` and `prepare_managed_workflow_inner` |
| `stack_seed_validation.rs` | `validate_stack_seed_base_session`, `session_repo_is_in_project` (public) |
| `stack_child_spawn.rs` | the `StackChildSpawnHandler` struct; its `impl` is `child_spawn_handler.rs` |
| `conversation_spawn.rs` | `recipe_enables_conversation_spawn`, `conversation_branch_slug`, `GrillMeConversationSpawnHandler`; its `impl` is `conversation_spawn_handler.rs` |
| `roster_replacement.rs` | `roster_replacement_pairs` (public): the one source of what a session's roster withdraws, used by every sandboxed spawn and relaunch |
| `claude_cli_spawn.rs` | `spawn_claude_cli_session_inner`, the non-sandboxed Claude CLI spawn, with its steps in `claude_cli_spawn/claude_cli_spawn_steps.rs` (`ClaudeCliWorktreeCut`, `ManagedClaudeCliLaunch`, `ClaudeCliProcess`) |
| `session_worktree_observer.rs` | the `SessionWorktreeObserver` port, `DaemonSessionHost::with_worktree_observer` and `announce_worktree_ready` |
| `hooks_and_urls/daemon_urls.rs` | `advertise_daemon_url`, `local_daemon_hook_url`, `claude_hook_daemon_url` and their `DEFAULT_WEB_PORT`: where a hook command reaches this daemon and the URL it advertises to peers, each a function of the `DaemonConfig`. `hooks_and_urls.rs` re-exports it (`pub(crate)` module), and `cursor_cli_spawn/chat.rs` names it as `connection_service::daemon_urls` |
| `split_context_from_codebase_host_tests.rs` | the test of the split agent's context read against a stalled checkout. It builds a `DaemonSessionHost`, so it is a `#[cfg(test)]` sibling declared in `connection_service.rs`, not an inline block of `svc_split_context_from_codebase_host.rs` |

The attachment progress sink and reporter, `AttachmentMaterialization` and the cleanup are
`tddy_session_files::attachment_progress`, brought into the host's scope by
`pub(crate) use tddy_session_files::attachment_progress::*;` in `connection_service.rs`.

`activity_delta_frames` stays in `connection_service.rs` as a public one-line delegate to
`tddy-session-activity`'s function, through the `measured_delta` conversion both callers share
(`svc_activity_ports.rs`). Its frame-headroom `const _: assert!` is kept beside it.

### Session start

`svc_start_session_core.rs` holds `start_session_core`, the one implementation behind
`StartSession` and `StreamStartSession`. It checks the request, routes it, and dispatches by
placement and session type. Its children hold the steps:

| Module (`svc_start_session_core/`) | Holds |
|---|---|
| `start_request_checks.rs` | `forward_start_session` (a start owned by a peer), `provision_project_for_start`, `validate_stack_seed_against_project` |
| `workspace_branch_start.rs` | `seed_and_start_workspace_session` |
| `cli_branch_starts.rs` | the claude-cli and cursor-cli branches: `cli_start_prelude` (sessions base, new id, attachments and initial prompt, returned as a `CliStart`), `attached_initial_prompt` (shared with the split agent), and the four `*_from_request` starts |
| `tool_session_spawn.rs` | `spawn_tool_session` and `spawn_tddy_coder`: the one `tddy-coder` spawn, for both a start and a resume |
| `tool_spawn_plan.rs` | `ToolSpawnPlan` (what the child is spawned with) and `ToolSpawnPurpose` (`Start` / `Resume`: the deadline label, the log lines, and whether the worker traces itself) |

`managed_recipe_for` (the request's managed-workflow recipe, an unknown one refused) is a free
function in `svc_start_session_core.rs`.

### The sandboxed CLI starts and relaunch

| File | Holds |
|---|---|
| `svc_start_sandboxed_claude_cli_session.rs` | `start_sandboxed_claude_cli_session`, and the file-local parameter structs `JailSession<'a>`, `JailBranch<'a>`, `JailDirs`, `JailLaunch`, `JailRunnerEnv<'a>`, `type ManagedJailEnv` |
| `…/jail_launch_steps.rs` | `warm_up_jail_agents`, `managed_jail_env`, `jail_semantic_index_env`, `launch_jail` |
| `…/jail_session_files.rs` | `prepare_jail_dirs`, `prepare_jail_context_dir`, `write_jail_session_metadata` |
| `…/jail_worktree.rs` | `project_default_branch_ref`, `create_jail_project_worktree`, `link_jail_branch_to_stack_node` |
| `svc_relaunch_sandboxed_runner.rs` | `relaunch_sandboxed_runner`, with `RelaunchJailEnv`, `RelaunchedRunnerSpawn`, `RelaunchedJailBridge`, `type RelaunchManagedEnv` |
| `…/relaunch_jail_dirs.rs` | `prepare_relaunch_dirs`, `refresh_relaunch_context_dir` |
| `…/relaunch_jail_steps.rs` | `relaunch_managed_workflow`, `resolve_relaunch_binaries`, `relaunch_jail_env`, `spawn_relaunched_runner`, `bridge_relaunched_jail` |
| `svc_start_sandboxed_cursor_cli_session.rs` | `start_sandboxed_cursor_cli_session`, in one function |
| `svc_start_sandboxed_claude_cli_session/jail_env_builders.rs` | `specialized_subagent_env`, `jail_daemon_identity_env`, `lsp_tools_env`, which all three use |

The Claude and relaunch paths are cut into matching steps. The three paths are still separate
copies of one launch sequence. Merging them waits on test coverage:
[`docs/dev/todo/2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md`](../../../docs/dev/todo/2026-09-24-lifecycle-shared-sandboxed-jail-launch-needs-coverage-first.md).

### Split sessions

| File | Holds |
|---|---|
| `svc_spawn_split_agent.rs` | `spawn_split_agent` (the agent half of a split session), cut into `join_split_livekit_room`, `split_agent_context_and_args` and `write_split_agent_metadata`, with `SplitAgentProcess<'a>` |
| `svc_spawn_split_agent/svc_paired_codebase_teardown.rs` | `delete_paired_codebase_session`: tearing down the paired codebase session |
| `split_start/split_claude_cli_start.rs` | `start_split_claude_cli_session` |
| `svc_split_context_from_codebase_host.rs` | the split agent's context from the codebase host |
| `svc_start_sandboxed_codebase_session.rs` | a workspace start combined with `spawn_split_agent` |

### Session RPC handlers

`session_coordinate_handlers.rs` holds list, start, connect, the worktree snapshot and the streamed
start. Its children are `session_coordinate_handlers/svc_resume_session.rs`
(`resume_session_at_session_coordinate`) and `svc_signal_delete_session.rs`. `daemon_rpc_handler.rs`
dispatches `handle_rpc`.

### Agent clones and roster

The agent topic (topic 3 below) runs over [`AgentRoster`](#per-topic-state-and-the-topics-that-name-no-host),
not over `DaemonSessionHost`. Its modules name neither the host nor a wiring module; what they need
from the host that is not a field they reach through `AgentHostCallbacks`.

| File | Holds |
|---|---|
| `agent_host_callbacks.rs` | the `AgentHostCallbacks` trait, the `AgentRoster` handle and its `state()` |
| `svc_provision_agent_clone.rs` | `AgentRoster`'s provisioning and tear-down of agent clones, the roster broadcast and publish, and the clone and worktree lookups (12 methods) |
| `svc_start_hosted_agent_clone.rs` | starting a hosted clone, the unready and departed-daemon refusals, the forwarded and local conversation opens, and the clone's codebase access (9 methods) |
| `svc_ensure_session_room_for_agents.rs` | claiming, seeding and unwinding a clone's roster entry, and `ensure_session_room_for_agents` (6 methods). The file also holds two host methods of other topics: `provision_workspace_tool_sandbox` and `index_workspace_worktree` |
| `svc_resolve_listed_worktree.rs` | agent-def resolution (`resolvable_agent_defs`, `agent_def_for_spawn`, `resolve_specialized_agent_defs`), `seeded_roster_records`, `roster_session_dir` and `report_shadowed_agent_def` (6 methods), beside the host's `ensure_project_available_for_start` |
| `svc_turn_end_reporter.rs` | the cancel forward to an agent's owning daemon and the local and remote roster records for an agent id (4 methods) |
| `agent_roster.rs` | the free roster functions (`workspace_start_request`, `started_roster_rev`, `dispatch_envelope`, `refuse_unenforceable_withdrawal`, `session_enforces_a_withdrawal`, `roster_agent_ids`, `agent_tool_reads_the_clone`), the `agent_records` re-export and `split_forward_deadline(config)` |
| `peer_session_answer.rs` | see the table above; it also holds `resolve_worktree_root_in_session_dir` |
| `seeded_clone_guard.rs`, `seed_codebase.rs`, `roster_replacement.rs` | the guard that releases a seeded roster on drop (over the `AgentRoster` handle), `SeedCodebase` (the codebase a starting session seeds its roster from), and the roster's withdrawals |
| `session_dir_lookup.rs` | the free function `session_dir_for(tddy_data_dir, session_id)` |

The 37 methods are `impl AgentRoster`, so they read the host's field names unchanged, and a `self.clone()`
handed to a task clones the handle. The host keeps the wiring (all under `connection_service/`):

| File | Holds |
|---|---|
| `svc_agent_host_ports.rs` | `impl AgentHostCallbacks for DaemonSessionHost`, the one implementation of the trait. Its child `svc_agent_host_ports/session_room_opening.rs` holds the host's `ensure_session_room`: the room roster and the host's terminal bridge, which the topic reaches through the callback |
| `svc_agent_roster_wiring.rs` | `DaemonSeedCloneClaimant` (holds the `AgentRoster` handle) and `impl RemoteSnapshotSource for DaemonSessionHost`, the source of the callback `worktree_snapshot` |
| `svc_agent_roster_delegators.rs` | the seven one-line host delegators to the handle that a consumer or an in-crate test calls: `session_room_participant_identities`, `agent_clone_worktree_path`, `agent_clone_divergences`, `resolvable_agent_defs`, `agent_def_for_spawn`, `resolve_specialized_agent_defs`, `seeded_roster_records` |

`handler_state.rs` builds the handle (`DaemonSessionHost::agent_roster()`), and `svc_host_builders.rs`
builds the seed-clone claimant. The session-agents port adapters hold the handle and call it
directly.

### Ports, and the peer-routed wrappers

| File | Holds |
|---|---|
| `svc_session_agent_ports.rs` | the host's session-agents impl |
| `svc_session_agent_ports/svc_session_agent_port_adapters.rs` | the five port adapters, which call host methods |
| `svc_session_agent_ports/svc_peer_routed_session_agents.rs` | `PeerRoutedSessionAgents`: routes on `daemon_instance_id` and forwards the request itself to the owning peer |
| `svc_session_files_ports.rs` | the host's session-files impl |
| `svc_session_files_ports/svc_peer_routed_session_files.rs` | `PeerRoutedSessionFiles` |
| `svc_activity_ports.rs`, `svc_terminal_ports.rs`, `svc_demo_vm_ports.rs`, `svc_session_lifecycle_ports.rs` | the other families' ports |

The `PeerRouted*` wrappers stay in this crate rather than in `tddy-session-agents` and
`tddy-session-files`: the routing needs the eligible-daemon roster, the common room slot and the
LiveKit forwarding clients, and those crates deliberately do not grow a transport.

### The host builders

`svc_resolve_tddy_tools_path.rs` (49 lines) holds `resolve_tddy_tools_path`. The host constructor
`DaemonSessionHost::new` and every `with_*` / `set_*` builder are in `svc_host_builders.rs`, beside the
struct they build, with one child, `rpc_activity.rs` (`record_rpc_activity`). The free function
`mint_first_admission_token` is `connection_service/first_admission_token.rs`, a direct child of
`connection_service`.
`PresenterObserverDeps` and its `maybe_spawn_presenter_observer` are in
`presenter_observer_task/presenter_observer_spawn.rs`, under the module that runs the task they start.
The builders' file also holds the "sandbox RPC bridge not installed" refusal a sandboxed start meets when the
runtime never called `install_sandbox_rpc_bridge`.

### Other modules cut out of their topic's neighbours

Two modules that belong to topics already moved out of the crate, or about to, sit under the file
they were cut from, because their destination is a receiver crate and not a parent here:

| Module | Holds |
|---|---|
| `svc_resolve_os_user/os_user_resolution.rs` | the free function `resolve_os_user(config, user_resolver, session_token)`, re-exported by `svc_resolve_os_user.rs` as `connection_service::resolve_os_user` |
| `svc_host_builders/rpc_activity.rs` | `record_rpc_activity` |

The other modules that were cut from a neighbour's file sit under the parent of their topic:

| Module | Parent | Holds |
|---|---|---|
| `svc_materialize_staged_attachment/session_attachment_materialization.rs` | `svc_materialize_staged_attachment` | `impl AttachmentState`: `prepare_session_attachments`, `materialize_session_attachments` |
| `local_exec_tools/local_exec_tool_dispatch.rs` | `local_exec_tools` | `run_exec_tool_locally` |
| `svc_agent_host_ports/session_room_opening.rs` | `svc_agent_host_ports` | `ensure_session_room` |
| `split_start/split_claude_cli_start.rs`, `svc_start_sandboxed_claude_cli_session/jail_env_builders.rs`, `presenter_observer_task/presenter_observer_spawn.rs` | `split_start`, `svc_start_sandboxed_claude_cli_session`, `presenter_observer_task` | see the tables above |

## Per-topic state, and the topics that name no host

Five topics, and the leaves beside them, run without `DaemonSessionHost`. Each reads the host fields
its bodies need through a value the host builds in `connection_service/handler_state.rs`, or takes
those fields as parameters:

| Topic | What its bodies take | Defined in | Built by `DaemonSessionHost::…` |
|---|---|---|---|
| Demo VM | `DemoVmState`: `demo_vm_state` (the per-session VM table, shared with the host), `tddy_data_dir`, `user_resolver`, `rpc_activity` (shared) and `config` (a clone) | `activity_hub.rs`, which also holds `DemoVmHandle`. `impl DemoVmState` in `demo_vm_coordinate_handlers.rs` (start, stop and status at a coordinate) | `demo_vm_service_state()`. `DemoVmServiceImpl` holds the state; its constructor `DemoVmServiceImpl::new(Arc<DaemonSessionHost>)` is public and takes the host, so it stays in `svc_demo_vm_ports.rs` beside `demo_vm_entry` |
| Presenter observer | `PresenterObserverDeps`: `tddy_data_dir`, `presenter_event_sink` and `session_notification_bus` (both shared) | `presenter_observer_task/presenter_observer_spawn.rs`, with `maybe_spawn_presenter_observer`. `presenter_observer_task.rs` and `presenter_intent_client.rs` take plain arguments | `presenter_observer_deps()`; `maybe_spawn_presenter_observer` stays a host method that delegates |
| Attachments | `AttachmentState<'a>`, borrowed for one call: `config`, `tddy_data_dir`, `staging_base_dir`, `peer_routing`. No hand-off to a task needs an owned form | `svc_materialize_staged_attachment.rs`, with the staged-file and host-document materializers. `session_attachment_materialization.rs` has `prepare_session_attachments` and `materialize_session_attachments` | `attachment_state()`; `prepare_session_attachments` stays a host method that delegates |
| Agent clones and roster | `AgentRoster`: the host's roster fields under the host's names (`config`, `tddy_data_dir`, `user_resolver`, `peer_routing`, `room_roster`, `session_rooms`, `session_agent_rosters`, `session_agent_clones`, `hosted_agent_clones`, `roster_keepalive_interval`, `session_admissions`, `model_registry`), owned, plus `host: Arc<dyn AgentHostCallbacks>`. It is `Clone`, because a claim, a seeded-roster guard and a codebase-access closure hand it to a `'static` task. `state()` lends the twelve fields as `tddy_session_agents::AgentRosterState<'_>` | `agent_host_callbacks.rs`, with the 37 methods in the files of [Agent clones and roster](#agent-clones-and-roster) | `agent_roster()`; `AgentHostCallbacks` has five methods: `worktree_snapshot`, `run_exec_tool_locally`, `ensure_session_room`, `hosted_clone_for`, `run_hosted_clone_tool` |
| Admission token and OS user | no state value: free functions of the two or three fields they read | `mint_first_admission_token(config, session_admissions, session_id, owning_daemon_instance_id)` in `connection_service/first_admission_token.rs`; `resolve_os_user(config, user_resolver, session_token)` in `os_user_resolution.rs` | `mint_first_admission_token` (`handler_state.rs`) and `resolve_os_user` (`svc_resolve_os_user.rs`), both delegators |

Two more host methods are free functions of the fields they read, and stay as host methods that
delegate (`handler_state.rs`) because other host methods or a lifecycle test call them:
`split_forward_deadline(config)` (in `agent_roster.rs`) and `session_dir_for(tddy_data_dir,
session_id)` (`session_dir_lookup.rs`).

`session_notifications.rs` is a facade: it re-exports `tddy_session_activity::session_notifications`
and the `pub(crate)` module `session_notification_publishing` (`SessionNotificationPublishing`,
`resolve_session_label`: the publish context built on a session's display label, which is read
through `session_list_enrichment`). Call sites keep naming `crate::session_notifications::X`.

**No `DaemonSessionHost` in these files:** `activity_hub.rs`, `demo_vm_coordinate_handlers.rs`,
`presenter_observer_spawn.rs`, `presenter_observer_task.rs`, `presenter_intent_client.rs`,
`first_admission_token.rs`, `agent_host_callbacks.rs`, `agent_roster.rs`, `peer_session_answer.rs`,
`seeded_clone_guard.rs`, `seed_codebase.rs`, `roster_replacement.rs`, `svc_provision_agent_clone.rs`,
`svc_start_hosted_agent_clone.rs`, `svc_turn_end_reporter.rs`, `os_user_resolution.rs`, `svc_materialize_staged_attachment.rs`,
`session_attachment_materialization.rs`, `session_dir_lookup.rs`, `session_notifications.rs`,
`session_notification_publishing.rs`, and the leaves `placement.rs`, `remote_git_pack_execution.rs`,
`agent_list_mapping.rs` and `hooks_and_urls/daemon_urls.rs`. In the two mixed files
(`svc_ensure_session_room_for_agents.rs`, `svc_resolve_listed_worktree.rs`) only the agent topic's `impl AgentRoster`
and free functions are host-free; their other host methods stay `impl DaemonSessionHost`. The leaves and the state-taking topics
name foundations and receivers by their defining crate (`tddy_daemon_kernel::config::DaemonConfig`,
`tddy_daemon_livekit::peer_routing::PeerRouting`), not through a lifecycle facade.

Two files carry the host by necessity: `svc_demo_vm_ports.rs` (the public
`DemoVmServiceImpl::new` and `demo_vm_entry`, which are wiring) and `handler_state.rs` (the
builders and delegators above). Every topic not in this section (split
sessions, the CLI and sandboxed starts and resumes, stack spawns and the session-coordinate
handlers) is still `impl DaemonSessionHost` code, apart from what it calls on the agent topic.

## Shared helpers: `connection_service/service_util.rs`

Each of these is the one definition several start, resume and spawn paths call:

| Helper | What it does | Callers |
|---|---|---|
| `find_registered_project`, `project_repo_root` | the project registered for an OS user, and the projects directory it was found in; the project's checkout, refused if missing | every start path, 9 lookups |
| `starting_session_metadata` | the `SessionMetadata` a starting session is written with | 7 start paths |
| `write_initial_changeset` | resolve the branch intent and write the session's first changeset | the four CLI starts |
| `create_session_worktree` | cut the session's worktree, with its optional chain base, under the spawn deadline | four starts. `workspace_session` keeps its own: it maps its timeout differently and logs nothing |
| `index_session_worktree` | build the session's semantic index over its worktree, blocking until it is terminal. A missing embedder or a failed index is an error, with no unindexed fallback | all five indexing paths |
| `spawn_blocking_with_timeout`, `await_supervised_with_timeout` | a blocking or supervised task under a deadline (public, for the RPC handlers above this crate) | the list, start and resume paths |
| `push_new_branch_to_origin_if_requested`, `resume_agent_and_recipe` | the push a start may request; the agent and recipe a resume restores | start and resume |
| `write_claude_hooks_settings`, `resolve_start_session_claude_binary` | the `.claude/settings.local.json` that wires a session's lifecycle hooks into the directory `claude` runs in (warn and continue); the `claude` binary the interactive start runs, through `config::resolve_claude_binary_path` so the interactive and sandboxed paths never pick differently | the non-sandboxed Claude spawn and its steps, the split agent spawn |

The "trim, and treat empty as unset" conversion is `tddy_daemon_kernel::trim_to_option`, used at
every site in this crate.

## Other directory modules

| Module | Children |
|---|---|
| `cli_session_manager` | `CliSessionManager`, the PTY session manager and the origin of `TaskRegistry`. `cli_session_manager.rs` (173 production lines) holds the struct, `ControlLeaseInfo` and `LiveKitTerminalAddress`; its `impl` blocks are `pty_handle.rs` (`PtyHandle`, `send_input`), `control_lease.rs`, `argv.rs`, `launch.rs`, `pty_spawn.rs`, `relaunch.rs`, `terminals.rs`, `livekit_terminals.rs` and `livekit_bridge.rs` (serves `terminal.TerminalService` against a PTY handle over LiveKit) |
| `cursor_cli_spawn` | `spawn_cursor_cli_session_inner` (public module; a wrapper over `spawn_cursor_cli_session_reporting`, which reports the start's phases), with `CursorCliSessionRecord<'a>`; `chat.rs` (hooks, `parse_created_chat_id`, `mint_cursor_chat_id`) and `resume.rs` (`resume_cursor_cli_session`) |
| `split_session` | split-session start; `agent_argv.rs` (native-tool constants, roster withdrawals, `split_claude_extra_args`) and `agent_credentials.rs` (the token TTL, `mint_agent_session_token`, `verified_caller`, `RoomPollTokenMinter`). `PERMISSION_PROMPT_TOOL` comes from `tddy-sandbox-recipes` |

## Modules that live below this crate, and their facades

These topics have no host state, so they live in the crate that owns their subject. Each keeps its
`tddy_session_lifecycle::…` path through a facade in `lib.rs` (or, for a `pub(crate)` module, in
`connection_service.rs`), so `tddy-daemon-rpc`, `tddy-daemon` and `tddy-telegram-control` name them
exactly as they name this crate's own modules. No receiver depends on this crate, normal or dev.

| Module | Lives in | Facade here |
|---|---|---|
| `task_service`, `action_service` | `tddy-daemon-sandbox` | `pub use tddy_daemon_sandbox::*;` |
| `relay_idle` (`RpcActivity`), `local_token_tonic_adapter` | `tddy-daemon-kernel` | `pub use tddy_daemon_kernel::*;` |
| `pty_runtime`, `tddy_user_config` | `tddy-terminal-rpc` | `pub use tddy_terminal_rpc::{pty_runtime, tddy_user_config};` |
| `session_reader`, `user_sessions_path`, `session_deletion`, `session_list_enrichment` | `tddy-session-activity` | `pub use tddy_session_activity::{session_deletion, session_list_enrichment, session_reader, user_sessions_path};` |
| `peer_routing` (`PeerRouting`), `session_admission_service` | `tddy-daemon-livekit` | `pub use tddy_daemon_livekit::{peer_routing, session_admission_service};` |
| `attachment_progress` | `tddy-session-files` | `pub(crate) use tddy_session_files::attachment_progress::*;` in `connection_service.rs` (no public path) |

A facade is named rather than globbed wherever the receiver has a module this crate also declares:
a root glob of `tddy-terminal-rpc` would re-export its `service`, which this crate's private
`mod service;` shadows (`hidden_glob_reexports`).

**The agent roster's host-free code is in `tddy-session-agents`.** The roster methods here
(`impl AgentRoster`) call it; seven of them also have a host delegator, so every public method keeps
its path. What they call lives in the receiver:

| `tddy_session_agents::…` | Called by `AgentRoster`'s |
|---|---|
| `clone_readiness::refuse_unready_clone` | `refuse_unready_clone` |
| `agent_clone_lookup::agent_clone_for` | `agent_clone_for` |
| `agent_clone_worktree::agent_clone_worktree_path` | `agent_clone_worktree_path` |
| `conversation_cancel_forward`, `conversation_open_forward` | `forward_cancel_agent_conversation`, `forward_open_agent_conversation` |
| `departed_daemon::refuse_departed_daemon` | `refuse_departed_daemon` |
| `session_room_participants::session_room_participant_identities` | `session_room_participant_identities` |
| `roster_broadcast::broadcast_roster` | `broadcast_roster` (the `let … else` room lookup stays here) |
| `opened_session_room::require_opened_session_room` | `ensure_session_room_for_agents` |
| `spawn_agent_def::agent_def_for_spawn` | `agent_def_for_spawn` |
| `hosted_clone_start::start_hosted_agent_clone` | `start_hosted_agent_clone` (the head, which calls `workspace_session`, stays here) |
| `agent_records::{def_tool_names, roster_record, qualified_agent_id, started_agent_id}` | `agent_roster.rs`, which re-exports them (`pub use tddy_session_agents::agent_records::*;`) |
| `exec_tool_caller::authorize_exec_tool_caller` | `resolve_exec_tool_worktree`; re-exported as `connection_service::authorize_exec_tool_caller` |

Where a moved body reads host fields, it takes `tddy_session_agents::AgentRosterState<'a>`, a view
of the twelve roster fields borrowed for one call, which `AgentRoster::state()` lends. The demo VM,
the presenter observer and attachment materialization take the same kind of view (see
[Per-topic state](#per-topic-state-and-the-topics-that-name-no-host)). The agent topic has one
callback trait, `AgentHostCallbacks`; the topics that need a host capability beyond a field (split
and launch) are still host methods.

## Definitions this crate takes from below

| From | What | Instead of |
|---|---|---|
| `tddy-pty` | `strip_resize`, the decoder for the in-band OSC resize `\x1b]resize;{cols};{rows}\x07` a terminal client writes into a PTY's input | a copy in `cli_session_manager`. `tddy-coder`'s session participant and `tddy-sandbox-runner` use the same one |
| `tddy-worktree-service` | `MpscResultStream` (with `into_receiver`), re-exported at `connection_service::MpscResultStream` | a copy in `connection_service.rs` |
| `tddy-sandbox-recipes` | `PERMISSION_PROMPT_TOOL` | a copy in `split_session.rs` |
| `tddy-session-activity` | the activity delta framing behind `activity_delta_frames` | a stale copy |
| `tddy-daemon-kernel` | `trim_to_option` | ten inline copies |

## Topics

The crate declares no topics. This grouping comes from module docs and function names. The column
**Where** lists what is in this crate; the modules each topic has below it are in the table above.

| # | Topic | Where |
|---:|---|---|
| 1 | Agent CLI start and resume: Claude and Cursor in PTYs, the sandboxed variants, `tddy-tools` path, hooks, local exec tools, agent-def resolution | `claude_cli_spawn`, `cursor_cli_spawn`, `svc_start_*_cli_session*`, `svc_relaunch_sandboxed_runner`, `svc_resume_claude_cli_session`, `hooks_and_urls`, `local_exec_tools`, `jail_relaunch` |
| 2 | RPC host core: `DaemonSessionHost`, the `session.SessionService` handlers and adapter, the RPC-families port, shared helpers | `connection_service.rs`, `session_coordinate_handlers`, `daemon_rpc_handler`, `service_util`, `svc_host_builders`, `rpc_families`, `handler_state` |
| 3 | Agent clones, roster and multi-agent rooms | `agent_host_callbacks`, `agent_roster`, `roster_replacement`, `svc_provision_agent_clone`, `svc_start_hosted_agent_clone`, `svc_ensure_session_room_for_agents`, `svc_resolve_listed_worktree`, `svc_turn_end_reporter`, `seeded_clone_guard`, `seed_codebase`, `peer_session_answer`, `session_dir_lookup`; its wiring is `svc_agent_host_ports`, `svc_agent_roster_wiring` and `svc_agent_roster_delegators`; the host-free parts are in `tddy-session-agents` |
| 4 | Split and sandboxed-codebase sessions | `split_session`, `split_start`, `svc_spawn_split_agent`, `svc_split_context_from_codebase_host`, `svc_start_sandboxed_codebase_session` |
| 5 | Session catalog: list with enrichment, read, delete, notifications, workspace sessions | `session_notifications` (a facade) and `session_notification_publishing` (`SessionNotificationPublishing`), `workspace_session`; listing, reading and deletion are in `tddy-session-activity` |
| 6 | Terminals, PTY runtime, and the tasks and actions RPCs | `cli_session_manager`, `terminal_session_adapter`, `svc_terminal_ports`; the PTY runtime is in `tddy-terminal-rpc`, the tasks and actions services in `tddy-daemon-sandbox` |
| 7 | Routing, peers, OS user, room admission, local token, relay idle | `svc_resolve_os_user` (the host's routing delegations and the host's `resolve_exec_tool_worktree`), `peer_session_answer` (the free `resolve_exec_tool_worktree`, `peer_has_no_such_session`, `split_pairing`), `os_user_resolution`, `first_admission_token`; routing and admission are in `tddy-daemon-livekit`, the local token and relay idle in `tddy-daemon-kernel` |
| 8 | Attachments and session files | `svc_materialize_staged_attachment`, `svc_materialize_staged_attachment/session_attachment_materialization`, `svc_session_files_ports`; the progress types are in `tddy-session-files` |
| 9 | Stacked, child and conversation spawns, and PR-stack links | `stack_parent`, `stack_seed_validation`, `stack_child_spawn`, `child_spawn_handler`, `conversation_spawn*`, `svc_pr_status_for_caller` |
| 10 | Activity ports and presenter observation | `svc_activity_ports`, `presenter_observer_spawn`, `presenter_observer_task`, `presenter_intent_client` |
| 11 | Demo VM | `activity_hub` (`DemoVmState`, `DemoVmHandle`), `demo_vm_coordinate_handlers`, `svc_demo_vm_ports` |

What is left of each topic here, apart from the agent roster, the demo VM, the presenter observer, attachments and
admission (see [Per-topic state](#per-topic-state-and-the-topics-that-name-no-host)), is `impl
DaemonSessionHost` code: an inherent `impl` of this crate's type cannot leave it (`E0116`), and most
of it calls other host methods or hands `self.clone()` to a task. Converting those methods in place
into functions over per-topic state and callback ports, and then moving each converted topic to its
receiver, is the rest of the `#carve` stack, which starts with moving the agent topic into
`tddy-session-agents`:
[#533](https://github.com/uppin/tddy-coder/pull/533) (split sessions),
[#534](https://github.com/uppin/tddy-coder/pull/534) (stack spawns and the jail and CLI launches),
[#535](https://github.com/uppin/tddy-coder/pull/535) (session start, resume and the coordinate
handlers) and [#536](https://github.com/uppin/tddy-coder/pull/536) (the moves that leave this crate a
wiring crate).

## Coupling

- **Inside the crate.** Every `svc_*` file is a child of `connection_service`, so it reads
  `DaemonSessionHost`'s private fields directly. Moving code between siblings needs `pub(super)` at
  most.
- **Out of the crate.** The private fields are the obstacle. Each topic's `impl` block reaches into
  one struct, so moving it to another crate first needs a per-topic state or port struct, the
  pattern `connection_service/handler_state.rs` sets (`agent_roster()` builds the handle of the topic
  bound for a receiver; `demo_vm_service_state()`, `presenter_observer_deps()` and
  `attachment_state()` are the ones built for topics that stay in this crate for now).
- **Reverse dependencies.** `tddy-daemon-rpc`, `tddy-daemon` and `tddy-telegram-control` depend on
  this crate. `tddy-model-registry`, `tddy-tool-engine` and `tddy-worktree-service` use it only as a
  dev-dependency.
- **Crates below this one** that can take code without a cycle: `tddy-session-agents`,
  `tddy-session-files`, `tddy-session-activity`, `tddy-session-catalog`, `tddy-terminal-rpc`,
  `tddy-worktree-service`, `tddy-daemon-kernel` and `tddy-daemon-livekit`. Anything that needs
  `DaemonSessionHost` itself cannot go below this crate without a port.
