# 2026-09-11 — The daemon's two remaining tonic adapters are still hand-written

**Category:** Future enhancement
**Source:** `#unbundle` node 6, [#475](https://github.com/uppin/tddy-coder/pull/475) — the node that
implemented the generator, changeset
[`2026-09-11-unbundle-session-io-services`](../changesets/2026-09-11-unbundle-session-io-services.md)

`tddy-codegen`'s `generate_tonic_adapter` now emits a complete tonic trait impl for all four method
shapes — unary, server-streaming, client-streaming and bidirectional — and
`terminal_session.TerminalSessionService`'s adapter is generated and carrying the jail's live terminal
traffic on the daemon's local UDS socket. See
[tonic-adapter.md](../../../packages/tddy-codegen/docs/tonic-adapter.md).

Two adapters were not regenerated:

| File | Methods |
|---|---|
| `packages/tddy-daemon/src/host_tonic_adapter.rs` | 8 |
| `packages/tddy-daemon/src/worktree_tonic_adapter.rs` | 9 |

They were left because regenerating them belonged to neither the PR that wrote them nor the PR that
wrote the generator: putting a rewrite of a predecessor's files into the generator's own diff makes
both harder to review. The follow-up is the cheapest kind — **its whole diff is a deletion**, plus
two `generate_tonic_adapter: true` / `tonic_trait_path: …` pairs in `packages/tddy-service/build.rs`
and the `use` line each mount needs.

`packages/tddy-daemon/src/connection_tonic_adapter.rs` is deliberately **not** on this list: it is
deleted outright when `connection.ConnectionService` stops being served on the socket.

## What to check when doing it

- Both protos need a **tonic pass** in `packages/tddy-service/build.rs` for the adapter to have a
  trait to implement, and `tonic_trait_path` must name tonic-build's own `<service>_server` module in
  whichever `OUT_DIR` sub-module that pass writes. `packages/tddy-terminal-rpc/build.rs` is the
  worked example of both passes over one proto.
- The hand-written pair already calls `tddy_service::to_tonic_status`, which is what the generator
  emits, so no refusal changes gRPC code.
- `packages/tddy-daemon/src/local_socket_server.rs` mounts all four services from one
  `Server::builder()`; only the adapter *types* it names change.
