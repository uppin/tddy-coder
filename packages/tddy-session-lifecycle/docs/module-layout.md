# Module layout

`tddy-session-lifecycle` is the daemon's **wiring crate**: it holds `DaemonSessionHost`, the builders
that assemble it, the port implementations that connect the host to the crates that own each topic, and
the facades that keep every `tddy_session_lifecycle::…` path resolving. It holds no topic code: every
topic lives in a receiver crate below it, and no receiver depends on this crate, normal or dev. The RPC
surface is [session-service.md](session-service.md); the test suites are [test-suites.md](test-suites.md).

## Sizes, and how they are counted

About **5.6k production lines** in 38 non-test `src/*.rs` files. The largest is `connection_service.rs`
(525, in [`code-issues/`](code-issues/oversized-file-connection-service.md)); every other file is under
500. One function is over 150 lines, `handle_rpc` in `daemon_rpc_handler.rs` (185, the RPC dispatch table),
in [`code-issues/`](code-issues/complexity-daemon-rpc-handler-handle-rpc.md). The size is above the ~4.5k the
wiring definition projected, by the 366-line `test_util` (ungated: the tests of `tddy-daemon` and
`tddy-daemon-rpc` name `tddy_session_lifecycle::test_util`, so a `test-util` feature would put feature lines
in their manifests) and by the facade lines for each receiver, in
[`docs/dev/todo/2026-10-08-session-lifecycle-test-util-is-not-gated-behind-a-test-util-feature.md`](../../../docs/dev/todo/2026-10-08-session-lifecycle-test-util-is-not-gated-behind-a-test-util-feature.md).

A **production line** is a line of a `src/` file outside every `#[cfg(test)] mod x { … }` block, with test
files excluded: those declared behind `#[cfg(test)] mod x;` and `*_tests.rs`. This is the
**inline-test-block rule**; the script that applies it is in the change history,
[`2026-09-23-carve-lifecycle-destructure`](../../../docs/dev/changesets/2026-09-23-carve-lifecycle-destructure.md#loc-assessment).
Counting only to a file's first `#[cfg(test)]` gets this crate wrong: `connection_service.rs` declares
`#[cfg(test)] use` lines near its top.

**The wiring definition:** no function in this crate does more than construct, delegate or implement a
port. The exceptions are listed under [What stays, and why](#what-stays-and-why).

## `connection_service`: the RPC host

`connection_service.rs` holds `DaemonSessionHost` (every field private), its `mod` declarations, and the
`pub use` blocks that re-export each receiver's modules at their old paths. Each `svc_*` file is a child
module, so it reads the host's private fields directly and needs `pub(super)` at most.

| File | Holds |
|---|---|
| `connection_service.rs` | the struct, the `mod` lines, the facade blocks ([facades](#modules-that-live-below-this-crate-and-their-facades)), `ROSTER_KEEPALIVE_INTERVAL` and the compile-time check that two keepalives fit inside a relay's idle deadline |
| `svc_host_builders.rs` (child `rpc_activity.rs`) | `DaemonSessionHost::new` and every `with_*` / `set_*` builder, and the "sandbox RPC bridge not installed" refusal a sandboxed start meets when the runtime never called `install_sandbox_rpc_bridge` |
| `handler_state.rs` | the per-topic builders: `agent_roster()`, `split_sessions()`, `launch_sessions()`, `attachment_state()`, `presenter_observer_deps()`, `demo_vm_service_state()`, and the free-function delegates `split_forward_deadline` and `session_dir_for` |
| `daemon_rpc_handler.rs` | `DaemonRpcHandler`'s `HostRpcHandler` impl: `handle_rpc` dispatches what a jail's bridge relays (family B, `SessionAgentService`), and `BoundJailSession` names the session the jail serves |
| `svc_agent_host_ports.rs` (child `session_room_opening.rs`) | the one `impl AgentHostCallbacks`, `impl SplitHost` and `impl LaunchHost` for the host, and the host's `ensure_session_room` |
| `svc_agent_roster_wiring.rs`, `svc_conversation_worktree_wiring.rs`, `svc_session_identity_wiring.rs`, `svc_worktree_observer_wiring.rs` | the host's `RemoteSnapshotSource` impl, its serving of a jail-relayed `ConversationWorktree` request, and the session-identity and worktree-observer methods (15 to 57 lines each) |
| `svc_agent_roster_delegators.rs`, `svc_launch_delegators.rs`, `svc_split_delegators.rs` | the one-line host methods a consumer or an in-crate test still calls, forwarding to the topic handle |
| `svc_resolve_os_user.rs` | the host's routing delegations and its `resolve_exec_tool_worktree`; `resolve_os_user` is the receiver's, re-exported here as `connection_service::resolve_os_user` |
| `local_exec_tools.rs` (child `local_exec_tool_dispatch.rs`) | where an exec tool runs on this daemon (a jail, a session's checkout, or a hosted agent clone), shared by the host and `tddy-daemon-rpc`'s exec-tool family |
| `svc_shut_down_children.rs`, `terminal_bridge_impl.rs` | shutdown of a host's children; the host's terminal bridge |
| `svc_demo_vm_ports.rs` | `DemoVmServiceImpl`'s wiring and `demo_vm_entry`; `DemoVmServiceImpl::new` takes the `DemoVmState` the host builds |

### Ports, and the peer-routed wrappers

| File | Holds |
|---|---|
| `svc_session_lifecycle_ports.rs` | the `SessionHandler` / `SessionService` impls, each entry a call to the launch handle |
| `svc_session_agent_ports.rs` | the host's session-agents impl |
| `svc_session_agent_ports/svc_session_agent_port_adapters.rs` | the five port adapters, which hold the `AgentRoster` handle and call it |
| `svc_session_agent_ports/svc_peer_routed_session_agents.rs` | `PeerRoutedSessionAgents`: routes on `daemon_instance_id` and forwards the request to the owning peer |
| `svc_session_files_ports.rs` | the host's session-files impl |
| `svc_session_files_ports/svc_peer_routed_session_files.rs` | `PeerRoutedSessionFiles` |
| `svc_activity_ports.rs`, `svc_terminal_ports.rs` | the activity and terminal families' ports |

The `PeerRouted*` wrappers stay in this crate rather than in `tddy-session-agents` and `tddy-session-files`:
the routing needs the eligible-daemon roster, the common room slot and the LiveKit forwarding clients, and
those crates deliberately do not grow a transport.

### Other files at the crate root

| File | Holds |
|---|---|
| `lib.rs` | the facades below, `SessionError`, and the `pub mod` declarations |
| `handler.rs`, `service.rs` | `SessionHandler`, `SessionStartEventStream`, `SessionServiceImpl`, `build_session_entry` |
| `rpc_families.rs` | the `DaemonRpcFamilies` port ([session-service.md](session-service.md)) |
| `terminal_session_adapter.rs` | the terminal RPC adapter over `CliSessionManager` |
| `session_agent_clone.rs` | `clone_worktree_path`, plus a re-export of `tddy_session_agents::session_agent_clone`; the function is unreferenced |
| `session_notifications.rs` | a facade over `tddy_session_activity::session_notifications` and `session_notification_publishing` |
| `claude_cli_session.rs`, `pr_stack_rpc.rs` | historical-path re-exports of `cli_session_manager` and `tddy_pr_stack::rpc` |
| `test_util.rs` | the shared fixtures of the integration suites (ungated; see above) |

## What stays, and why

- **The host, its builders and its port impls**: they name `DaemonSessionHost`'s private fields. A receiver
  cannot, so each topic sees the host through an owned handle it defines (`AgentRoster`, `SplitSessions`,
  `LaunchSessions`) and a callback trait (`AgentHostCallbacks`, `SplitHost`, `LaunchHost`) the host
  implements here.
- **`PeerRouted*` and the port adapters**: see above.
- **`local_exec_tools.rs`**: the local exec-tool dispatch, kept in the wiring crate with its
  `run_exec_tool_locally` child because the host and `tddy-daemon-rpc` share it.
- **`svc_resolve_os_user.rs`**: routing delegations and `resolve_exec_tool_worktree`, which read host fields.

## Per-topic state, and who defines it

Each topic reads the host fields its bodies need through a value the host builds in `handler_state.rs`.
The value and its methods are defined in the receiver, so an inherent `impl` moves with its type.

| Topic | State value | Defined in | Built by `DaemonSessionHost::…` |
|---|---|---|---|
| Agent clones and roster | `AgentRoster` (the twelve roster fields owned, plus `host: Arc<dyn AgentHostCallbacks>`) | `tddy-session-agents` (`agent_host_callbacks.rs`) | `agent_roster()` |
| Split sessions | `SplitSessions` (the host's split fields, the `AgentRoster` handle and `host: Arc<dyn SplitHost>`) | `tddy-session-split` (`split_ports.rs`) | `split_sessions()` |
| Launch | `LaunchSessions` (the host's launch fields and the topic handles) | `tddy-agent-launch` (`launch_ports.rs`) | `launch_sessions()` |
| Attachments | `AttachmentState<'a>`, borrowed for one call | `tddy-session-files` (`svc_materialize_staged_attachment.rs`) | `attachment_state()` |
| Presenter observer | `PresenterObserverDeps` | `tddy-session-activity` (`presenter_observer_spawn.rs`) | `presenter_observer_deps()` |
| Demo VM | `DemoVmState` | `tddy-demo-vm-service` (`activity_hub.rs`) | `demo_vm_service_state()` |
| Admission token, OS user | no state: free functions of the fields they read | `tddy-daemon-livekit` (`first_admission_token.rs`, `os_user_resolution.rs`) | `mint_first_admission_token` in `handler_state.rs` |

The three callback traits are implemented once, in `svc_agent_host_ports.rs`:
`AgentHostCallbacks` (the agent topic's), `SplitHost: AgentHostCallbacks` (`start_workspace_session`,
`delete_session`, `session_files`, `session_agents`) and `LaunchHost` (`sandbox_rpc_handler`, `pr_stack`,
`session_account_access`, `session_identity`).

## Modules that live below this crate, and their facades

Each topic keeps its `tddy_session_lifecycle::…` path through a facade in `lib.rs` (or, for a `pub(crate)`
module, in `connection_service.rs`), so `tddy-daemon-rpc`, `tddy-daemon` and `tddy-telegram-control` name
them exactly as they name this crate's own modules.

| Module | Lives in | Facade here |
|---|---|---|
| `cli_session_manager`, `session_toolcall` | `tddy-cli-sessions` | `pub use tddy_cli_sessions::{cli_session_manager, session_toolcall};` |
| `cursor_cli_spawn` and the other launch modules a consumer names | `tddy-agent-launch` | `pub use tddy_agent_launch::cursor_cli_spawn;`, and `pub use tddy_agent_launch::{…}` blocks in `connection_service.rs` |
| the split topic's public items (`resolve_tddy_tools_path`, `service_util`'s two `pub use`, `workspace_session`) | `tddy-session-split` | `pub use tddy_session_split::{…}` in `connection_service.rs` |
| `activity_hub`, `demo_vm_coordinate_handlers`, `demo_vm_service` | `tddy-demo-vm-service` | `pub use tddy_demo_vm_service::{…};` in `connection_service.rs` |
| `agent_list_mapping`, `config`, `daemon_hook_urls`, `relay_idle`, `local_token_tonic_adapter` | `tddy-daemon-kernel` | `pub use tddy_daemon_kernel::{agent_list_mapping, config};` and `pub use tddy_daemon_kernel::*;` |
| `first_admission_token`, `os_user_resolution`, `placement`, `peer_routing`, `session_admission_service`, the five LiveKit modules | `tddy-daemon-livekit` | `pub use tddy_daemon_livekit::{…};` |
| the ten session-file modules, `attachment_progress`, `svc_materialize_staged_attachment`, `session_attachment_materialization` | `tddy-session-files` | `pub use tddy_session_files::{…};`, and `pub(crate) use tddy_session_files::attachment_progress::*;` |
| `session_reader`, `user_sessions_path`, `session_deletion`, `session_list_enrichment`, `presenter_intent_client`, `presenter_observer_task`, `remote_git_pack_execution`, `session_notification_publishing`, `session_notification_subscribers` | `tddy-session-activity` | `pub use tddy_session_activity::{…};` |
| `agent_host_callbacks`, `agent_roster`, `roster_replacement`, `seeded_clone_guard`, `seed_codebase`, `peer_session_answer`, `session_dir_lookup`, `svc_provision_agent_clone`, `svc_start_hosted_agent_clone`, `svc_ensure_session_room_for_agents`, `svc_resolve_listed_worktree`, `svc_turn_end_reporter` | `tddy-session-agents` | `pub use tddy_session_agents::{…};` in `connection_service.rs` |
| `task_service`, `action_service` | `tddy-daemon-sandbox` | `pub use tddy_daemon_sandbox::*;` |
| `pty_runtime`, `tddy_user_config` | `tddy-terminal-rpc` | `pub use tddy_terminal_rpc::{pty_runtime, tddy_user_config};` |
| the worktree, host, project, auth and telegram modules | `tddy-worktree-service`, `tddy-host-service`, `tddy-projects`, `tddy-daemon-auth`, `tddy-telegram` | named `pub use` lines in `lib.rs` |

A facade is named rather than globbed wherever the receiver has a module this crate also declares: a root
glob of `tddy-terminal-rpc` would re-export its `service`, which this crate's private `mod service;`
shadows (`hidden_glob_reexports`). Four `#[cfg(test)]` glob re-exports in `connection_service.rs`
(`service_util::*`, `agent_roster::*`, `stack_child_spawn::*`, `conversation_spawn::*`) let the host's
in-crate tests name the moved items as they always did. 37 of the named facade modules have no user in this
crate or in a consumer; they are the node's public facade by contract.

The topic detail of each receiver is in its own docs: [`tddy-agent-launch`](../../tddy-agent-launch/docs/module-layout.md),
[`tddy-session-split`](../../tddy-session-split/docs/module-layout.md),
[`tddy-cli-sessions`](../../tddy-cli-sessions/docs/module-layout.md),
[`tddy-demo-vm-service`](../../tddy-demo-vm-service/docs/demo-vm-service.md),
[`tddy-session-agents`](../../tddy-session-agents/docs/agent-clone-roster.md),
[`tddy-session-files`](../../tddy-session-files/docs/attachment-materialization.md),
[`tddy-session-activity`](../../tddy-session-activity/docs/presenter-observer.md),
[`tddy-daemon-livekit`](../../tddy-daemon-livekit/docs/placement-and-admission.md) and
[`tddy-daemon-kernel`](../../tddy-daemon-kernel/docs/daemon-kernel.md).

## Definitions this crate takes from below

| From | What | Instead of |
|---|---|---|
| `tddy-pty` | `strip_resize`, the decoder for the in-band OSC resize `\x1b]resize;{cols};{rows}\x07` a terminal client writes into a PTY's input | a copy in the CLI session manager. `tddy-coder`'s session participant and `tddy-sandbox-runner` use the same one |
| `tddy-worktree-service` | `MpscResultStream` (with `into_receiver`), re-exported at `connection_service::MpscResultStream` | a copy in `connection_service.rs` |
| `tddy-session-activity` | the activity delta framing behind `activity_delta_frames` (a public one-line delegate here, through the `measured_delta` conversion both callers share; its frame-headroom `const _: assert!` stays beside it) | a stale copy |
| `tddy-daemon-kernel` | `trim_to_option` | ten inline copies |

## Coupling

- **Inside the crate.** Every `svc_*` file is a child of `connection_service`, so it reads
  `DaemonSessionHost`'s private fields directly.
- **Out of the crate.** Each topic crate sees the host only through its handle and its callback trait. No
  receiver names `tddy_session_lifecycle`; the crate graph has no edge into this crate from below
  (`cargo tree -i tddy-session-lifecycle -e normal,dev --workspace` lists `tddy-daemon`, `tddy-daemon-rpc`,
  `tddy-desktop`, `tddy-telegram-control` and the dev users `tddy-model-registry`, `tddy-tool-engine`,
  `tddy-worktree-service`).
- **Between receivers.** `tddy-agent-launch` depends on `tddy-session-split`, `tddy-cli-sessions`,
  `tddy-session-agents` and `tddy-session-files`; `tddy-session-split` depends on `tddy-cli-sessions`,
  `tddy-session-agents` and `tddy-session-files`, and not on `tddy-agent-launch`; `tddy-session-agents`
  depends on neither launch, split nor `tddy-cli-sessions`; `tddy-session-files` on none of the agents,
  split or launch crates; `tddy-cli-sessions` on neither split nor launch nor agents.
- **Reverse dependencies.** `tddy-daemon-rpc`, `tddy-daemon` and `tddy-telegram-control` depend on this
  crate; `tddy-desktop` builds on CI only.
