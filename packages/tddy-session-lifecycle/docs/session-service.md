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
