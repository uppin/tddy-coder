# 2026-09-11 — The coder participant dispatches `connection.ConnectionService` by string

**Category:** Future enhancement
**Source:** `#unbundle` node 6, [#475](https://github.com/uppin/tddy-coder/pull/475), changeset
[`2026-09-11-unbundle-session-io-services`](../changesets/2026-09-11-unbundle-session-io-services.md)

`packages/tddy-coder/src/session_participant/mod.rs`'s `SessionConnectionServiceRpc` implements
`tddy_rpc::RpcService` directly: `handle_rpc` takes the method as a `&str`, matches it against a list
of literals, decodes the request with `prost::Message::decode` by hand, and returns `Unimplemented`
for anything it does not recognise. Six methods are dispatched that way. It also **discards the
service name** it is handed, which is why the participant has to register two separate
`ServiceEntry`s — one service answering under two names would serve `ListExecTools` on the terminal
coordinate and `StreamTerminalOutput` on the connection one.

The generated `ConnectionService` trait exists and is what `tddy-daemon` implements. Its sibling
coordinate on the same participant already uses the generated path:
`packages/tddy-coder/src/session_participant/terminal_session_service.rs` implements
`tddy-terminal-rpc`'s ports and registers that crate's own entry constructor, which is exactly why
the two servers of the terminal family cannot drift.

## Why it matters

- **Nothing tells the coder a method exists.** A method added to `connection.proto` compiles fine
  here and silently answers `Unimplemented`. A trait impl would not compile until it was handled or
  explicitly refused.
- **A method removed from the proto still answers.** The match is on a string literal, so a
  coordinate the schema no longer declares keeps being served until someone notices.
- **Decode and refusal shapes are re-typed per arm**, so two arms can disagree about how a bad
  payload is refused.

## What closing it would take

Implement the generated `connection.ConnectionService` trait for the participant's service and
register it with the generated server, refusing the methods the coder does not serve explicitly —
`DeleteSession` / `SignalSession` are daemon-direct by design, and the bootstrap and directory
methods are the daemon participant's
([session-participant-rpc.md](../../ft/coder/session-participant-rpc.md)). The interesting part is
the refusals: the trait has ~50 methods and the coder serves six, so the shape of "explicitly not
served" has to stay readable rather than becoming 44 copies of one line.
