# 2026-09-15 — `RpcService` has no cancellation surface, so a handler cannot learn its caller is gone

**Category:** Future enhancement
**Source:** `2026-09-15-warm-code-intelligence-daemon` changeset — planning discovery

`tddy_rpc::RpcService` (`packages/tddy-rpc/src/bridge.rs:33-69`) passes a handler nothing but the
service name, the method name and the message. No `CancellationToken`, no deadline, no context.
`RpcMessage`'s metadata carries `sender_identity` and nothing else, and the only occurrence of
"deadline" in the crate is the `Status::deadline_exceeded` constructor (`status.rs:55`).

What exists instead is a **transport-level** abort. `ServerEngine` keeps each peer's in-flight
forwards and `on_peer_disconnected` aborts and awaits them
(`packages/tddy-rpc/src/server_engine.rs:212-235`), with the contract pinned by
`packages/tddy-rpc/tests/server_engine_peer_disconnect.rs:13`:

> The contract these tests pin: **when `on_peer_disconnected` returns, nothing further will be
> published for that peer**

That is about not publishing to a dead peer, not about stopping work. Two gaps follow:

- **It is not in every path.** `ServerEngine` backs LiveKit and the Tauri IPC bridge. The local UDS
  path serves generated tonic servers directly (`packages/tddy-daemon/src/local_socket_server.rs`),
  and `tddy-connectrpc` has no abort, disconnect or `Drop` handling at all — grepping its `src/` for
  `abort|disconnect|Drop` finds only the `Code::Aborted` string mapping in `error.rs:30`.
- **Aborting a future only stops it at an await point.** A handler that does its work inside
  `spawn_blocking` — which is how every synchronous engine in this repo is driven, including
  `tddy-code-restructuring`'s backend with its `std::thread::sleep` poll loops — runs the blocking
  closure to completion regardless.

So a ten-minute handler whose caller disconnected keeps a rust-analyzer query, a coverage capture or
a test binary running, on every transport, with no way for the handler to find out.

## Why it is recorded rather than fixed

The warm-index changeset needed exactly this and worked around it: each long request runs as a
`TaskBody` in a `TaskRegistry`, cancelled via `cancel_task`, with the token checked *inside* the
blocking loops, and a disconnect detected by a send failing into the response stream's dropped
receiver. That works, and it only works because those operations are server-streaming — a unary
handler has no back-channel at all.

Fixing it properly means a cancellation parameter on `RpcService`, which is a cross-cutting change
to a trait with many implementors across `tddy-service`, `tddy-terminal-rpc`, `tddy-sandbox-runner`,
`tddy-supervisor` and the generated servers in `tddy-codegen`. That belongs in its own change with
its own tests, not folded into a feature — the same reasoning
[2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md](./2026-08-14-no-livekit-rpc-call-has-a-client-side-deadline.md)
gives for the client-side half of the same problem. The two are the two ends of one gap and would
sensibly be done together.

Worth noting for whoever picks it up: `tokio-util` is **not** a workspace dependency. It is declared
per crate at `0.7` by `tddy-task` (`features = ["rt"]`), `tddy-vm`, `tddy-actions`,
`tddy-daemon-sandbox`, and by `tddy-core`, `tddy-coder`, `tddy-acp-stub` and
`tddy-integration-tests` with `features = ["compat"]`. `tddy-rpc` has no `tokio-util` dependency at
all, so a token on the trait adds one to the crate every transport depends on.
