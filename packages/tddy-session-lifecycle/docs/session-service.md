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

## Transports

`session.SessionService` registers on the daemon's HTTP `/rpc`, LiveKit common and session rooms, and
the local Unix socket (alongside the other services `runtime.rs` assembles).

Product docs: [claude-cli-session.md](../../../docs/ft/daemon/claude-cli-session.md),
[cursor-cli-session.md](../../../docs/ft/daemon/cursor-cli-session.md).
