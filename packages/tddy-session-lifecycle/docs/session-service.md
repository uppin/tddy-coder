# `session.SessionService` (tddy-session-lifecycle)

Eight RPCs over a session's whole life: listing, starting (unary or streamed with attachment
materialization), connecting, resuming, signalling, deleting, and measuring a checkout for a session
room. The proto is `packages/tddy-service/proto/session.proto`; handlers live under
`packages/tddy-session-lifecycle/src/` (the modules moved from `tddy-daemon` in `#unbundle` node 9).

## The surface

| RPC | Shape | What it does |
|---|---|---|
| `ListSessions` | unary | Sessions for the caller's OS user, enriched with agent status, activity and branch views |
| `StartSession` | unary | Start an agent session (non-interactive callers) |
| `StreamStartSession` | server stream | Same start path when attachments must be materialized before launch |
| `ConnectSession` | unary | Dial-in metadata for an existing session |
| `ResumeSession` | unary | Resume a stopped session |
| `SignalSession` | unary | Deliver a signal to a running session |
| `DeleteSession` | unary | Tear down a session |
| `GetWorktreeSnapshot` | unary | One checkout measurement for a session room (local or peer-fetched) |

`types.proto` types (`HostDocumentScope`, `SessionAgentStatus`, `SessionAgentActivity`,
`BranchSession`) are imported by this proto because listing and start reach them.

## Ownership

This crate owns **`TaskRegistry`**, which originates in `CliSessionManager` and was previously
re-exported through the dissolved `ConnectionServiceImpl`. Peer services that need the registry take
it from here, not from `tddy-daemon`.

The sandbox-IPC **`HostRpcHandler` bridge** lives in **`tddy-daemon-sandbox`** (not here): it is the
only caller that needed an `Arc` back into the old god object.

**This crate names nothing Telegram.** The Telegram control plane is
[`tddy-telegram-control`](../../tddy-telegram-control/README.md), which depends on this crate, and
`teloxide` is not in this manifest. `tddy-telegram` stays a dependency because
`session_list_enrichment` reads a session's pending elicitation through its `elicitation` module,
and `active_elicitation`, `elicitation`, `telegram_github_link` and `telegram_tracked_session` stay
re-exported here under their old paths.

## RPC families served above this crate

`DaemonSessionHost` implements the session family and the session-files, session-agents, activity,
terminal and demo-VM families. The **Project, Catalog, ExecTool and PR-stack** families are served
by handler structs in [`tddy-daemon-rpc`](../../tddy-daemon-rpc/docs/architecture.md), a crate that
depends on this one. This crate has no dependency on `tddy-daemon-rpc` of any kind, normal or dev.

### The `DaemonRpcFamilies` port

Session code needs those families in two places, and reaches them only through a port this crate
defines (`rpc_families.rs`, re-exported as `tddy_session_lifecycle::DaemonRpcFamilies`):

| Method | Used by |
|---|---|
| `pr_stack_handler() -> Arc<dyn PrStackHandler>` | session start, when its stack parent is owned by a peer (`resolve_chain_base_ref_status` → `resolve_stack_base`) or it names a stack node (`record_spawn_on_stack_node` → `link_stack_node`) — the same path, peer routing and refusals included, as a client's `ResolveStackBase` / `LinkStackNode` |
| `service_entries() -> Vec<ServiceEntry>` | `session_room_roster()`, which serves them beside this crate's own six family entries in every session room |

- `DaemonSessionHost::with_rpc_families(Arc<dyn DaemonRpcFamilies>)` installs it. The composition
  root calls it **last**, after every other `with_*`, through `tddy_daemon_rpc::RpcHandlers::install`:
  the handlers are built from this host's state and share it. The setters that feed handler state
  (`with_model_registry`, `with_credential_vaults`, `with_idle_tracker`,
  `with_eligible_daemon_source`) `debug_assert` that the port is not installed yet.
- `rpc_families()` returns the port, or **`FAILED_PRECONDITION`** naming the missing wiring when it
  was never installed. A host without it refuses the two paths above rather than opening a room that
  has silently lost four families or skipping a stack link. The port is a plain `Option` set at
  construction, not a late-bound `OnceLock`.
- **It is required only when a session room actually opens.** `SessionRoomRegistry::ensure_open` and
  `open_measured_by` take the roster as a builder and call it after the LiveKit-credentials check,
  so a daemon with no LiveKit credentials opens no room and never asks for its families —
  `ConnectSession` on such a host answers with an empty LiveKit block rather than refusing.

`test_util::RpcFamiliesNotUnderTest` is the fixture for a suite that opens a session room without
exercising any of the four families: its room serves none of them, and its `pr_stack_handler`
panics with a message sending the suite to `tddy-daemon-rpc`.

### Shared components

The behaviour the four families share with session code is exposed as small `pub` components, held
by both the host and the handlers. Each is shared, not copied: `Clone` hands out the same underlying
`Arc`s.

| Component | Where | What it is |
|---|---|---|
| `RpcActivity` | `relay_idle` | the daemon's idle tracker, when it has one; `record()` is what every RPC handler bumps so a relay daemon does not shut down mid-session. `RpcActivity::on(tracker)`, or `Default` for none |
| `PeerRouting` | `peer_routing` | this daemon's routing identity, the eligible peers and the common-room slot: `classify_addressed_daemon_route`, `common_room_slot`, `rpc_served_by_peer`, `eligible_daemon_source`, `common_room_livekit_room`. A session RPC and an exec-tool RPC addressed at the same daemon therefore agree on who owns the call |
| `LocalExecTools` | `connection_service` | where a tool runs on this daemon — a sandboxed session's jail, the session's own checkout, or a hosted agent clone — over the task registry, workspace sandboxes and hosted clones: `run_exec_tool_locally`, `run_hosted_clone_tool`, `hosted_clone_for`. A roster agent's turn loop and the `ExecuteTool` RPC take this one path |

Free functions over the fields they read, so a handler behaves exactly as the host does without
holding it (all re-exported from `connection_service`):

| Function | What it answers |
|---|---|
| `resolve_os_user` | a caller's session token → the OS user this daemon runs its work as |
| `authorize_exec_tool_caller` | authenticates an exec-tool caller before the hosted-clone branch, naming **this** daemon in both refusals |
| `resolve_exec_tool_worktree` | the worktree an exec-tool call resolves to |
| `resolve_tddy_tools_path` | the `tddy-tools` binary from the daemon's toolchain config — what session start and the catalogue's model probe both use |
| `resolvable_agent_defs` | YAML defs under `<tddy_data_dir>/agents` plus the model registry's assistants, the registry winning a name tie — the one list session start, roster attach and `ListSubagents` all resolve against |

`handler_state.rs` gives the handlers their inputs: `DaemonSessionHost::{config, user_resolver,
tddy_data_dir, eligible_daemon_source, spawn_client, common_room_livekit_room, model_registry,
credential_vaults, rpc_activity, peer_routing, local_exec_tools}`, each shared value handed out as
a clone of the same handle. `credential_vaults()` is the daemon's one `tddy_credentials::SessionVaults`
registry (re-exported as `tddy_daemon_auth::SessionVaults`), `None` when `auth_storage` is unset; the
PR-stack handler reads a caller's GitHub token from it.

`pr_stack_rpc` is a facade re-exporting `PrStackHandler`, `PrStackServiceImpl` and
`build_pr_stack_entry` from [`tddy_pr_stack::rpc`](../../tddy-pr-stack/docs/architecture.md#rpcrs--the-pr-stack-rpc-family),
also re-exported at the crate root. `family_proto_bridge::wire_same` and
`await_supervised_with_timeout` / `spawn_blocking_with_timeout` are `pub` for the handlers.

## The presenter observer

When a workflow session starts, `DaemonSessionHost::maybe_spawn_presenter_observer` calls
`presenter_observer_task::spawn_presenter_observer_task`, which connects to the child's
`PresenterObserver` gRPC stream (90 attempts, 100 ms apart) and hands each event to **two
independent sinks**:

| Sink | Held as | What it does |
|---|---|---|
| presenter-event sink | `presenter_event_sink: Option<SharedPresenterEventSink>` — the [`tddy-daemon-kernel` port](../../tddy-daemon-kernel/docs/daemon-kernel.md#ports) | Telegram's chat surface, on a daemon that has one |
| notification publishing | the host's session-notification bus, plus the caller's sessions base | publishes a `Presenter` notification so the session's drawer row shows a dot |

The observer is spawned when **either** exists and not at all when neither does. Gating it on
Telegram would leave the indicator dark on every daemon without a `telegram:` block.

`DaemonSessionHost::new` takes the sink as a parameter and installs **no** notification bus.
`tddy-daemon`'s `runtime.rs` installs the daemon's bus with `with_session_notification_bus`,
because the `StreamSessionNotifications` subscriber on it must be the very one the RPC handler
subscribes to. A test that needs a bus installs one the same way. With no bus, the observer runs
for the sink alone.

## Transports

`session.SessionService` registers on the daemon's HTTP `/rpc`, LiveKit common and session rooms, and
the local Unix socket (alongside the other services `runtime.rs` assembles).

Product docs: [claude-cli-session.md](../../../docs/ft/daemon/claude-cli-session.md),
[cursor-cli-session.md](../../../docs/ft/daemon/cursor-cli-session.md).
