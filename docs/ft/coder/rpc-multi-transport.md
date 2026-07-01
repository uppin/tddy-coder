# RPC Multi-Transport (stdio/IPC)

**Status:** ✅ Implemented (2026-07-01)

## Summary

The protobuf RPC framework used across tddy (`tddy-rpc` dispatch core + the LiveKit data-channel
transport in `tddy-livekit`) gains a second transport: **stdio/IPC**, for parent↔child process
communication. The same generated/hand-written RPC client and server code that runs over LiveKit
today can run unmodified over a child process's stdin/stdout, and either side of a stdio pipe can
initiate calls — not just the process that spawned the other.

## Background

`tddy-rpc` already separates RPC *dispatch* (`RpcService`, `RpcBridge<S>`, `RpcMessage`) from
*transport*. `tddy-connectrpc` proves this: it hosts the same `RpcBridge<S>` over HTTP/Connect.
`tddy-livekit` was the other existing transport, but (before this change) baked transport concerns
(LiveKit `Room`, `DataPacket`, participant identity) directly into both its client (`RpcClient`) and
server (`LiveKitParticipant`) — there was no client-side abstraction to swap the channel out.

Some tddy workflows need RPC between a parent process and a child process it spawns (e.g. a
sandboxed tool runner, a worker subprocess) without a LiveKit room in the loop. Today that would
mean either running LiveKit for a purely local process pair, or hand-rolling a bespoke protocol —
both worse than reusing the existing envelope and dispatch machinery.

## Requirements

1. **Same user code, either transport.** Code that calls or serves RPCs depends on transport-agnostic
   types (`RpcService` for serving; a client trait for calling) — not on `livekit::Room` or any
   stdio-specific type. Swapping LiveKit for stdio (or vice versa) requires no change to service
   implementations or call sites beyond how the channel is constructed.
2. **Multiplexed.** Many concurrent RPC calls (unary, server-streaming, client-streaming, bidi) run
   over one stdio pipe pair, correlated by request id — matching today's LiveKit behavior.
3. **Bidirectional.** Either peer on the stdio pipe can be the caller: the invoking process can call
   into the child, and the child can call back into the invoker, over the same pipe pair.

## Non-goals

- No TypeScript/Node stdio transport (Rust-to-Rust only for this change).
- No changes to the LiveKit wire format's compatibility with the existing TypeScript client
  (`tddy-livekit-web`).
- No changes to `tddy-connectrpc` (HTTP/Connect transport) or its consumers.

## Real consumers (2026-07-01 follow-up)

Beyond the `tddy-livekit` refactor this feature originally proved the transport with, real consumers
now exist: `tddy-coder`/`tddy-demo --stdio` and `tddy-sandbox-runner --stdio` both serve their
existing remote-control surfaces over this transport, and `tddy-tools`' sandbox tool-IPC dispatch
rides the same framing over a Unix socket (`StdioEndpoint::from_duplex`, added to wrap
already-open duplex streams a caller spawned itself — not just a `tokio::process::Command`
`spawn_child_endpoint` owns). See [grpc-remote-control.md](grpc-remote-control.md#stdio-transport)
and [tddy-sandbox architecture](../../../packages/tddy-sandbox/docs/architecture.md#control-channel-transport).

## Reference

Framing/multiplexing approach adapted from the `serial-comm` RPC-over-transport pattern documented
in the `makers-lt` reference repo (`docs/dev/serial-comm/architecture.md`,
`docs/dev/changesets/2026-01-13-rpc-over-serial-socket.md`): a protobuf request/response envelope
carrying a `request_id` for correlation, layered over a swappable framed byte channel.

## Implementation notes

**Architecture:** the envelope (`RpcRequest`/`RpcResponse`) and two transport-agnostic engines now
live in `tddy-rpc`: `client_engine::ClientEngine` (request-id correlation for outgoing calls) and
`server_engine::ServerEngine<S>` (dispatch + per-`(peer, request_id)` multiplexing for incoming
calls, including bidi sessions and client-streaming message accumulation). `tddy_rpc::RpcClientTransport`
is the object-safe client trait satisfying requirement 1 — both `tddy_livekit::client::RpcClient`
and `tddy_stdio::client::StdioRpcClient` implement it by delegating to `ClientEngine`, and either
can be used behind `&dyn RpcClientTransport` / generic `T: RpcClientTransport`. Each transport keeps
only what's genuinely transport-specific: LiveKit's multi-remote identity resolution and
reconnect-resilient publishing (`SharedPublisher`) live in `participant.rs`, wrapped *around*
`ServerEngine::on_request`, not inside it; stdio's length-prefixed frame codec
(`tddy_rpc::transport::{FrameKind, encode_frame, FrameDecoder}`) demultiplexes `Request` vs
`Response` frames on one duplex byte stream, which is what lets one peer be both client and server
over a single pipe.

**Real-time streaming protocol detail:** both server-streaming and bidi responses forward each item
immediately as it's produced (`ServerEngine::forward_response_body`) rather than buffering to look
one item ahead — a real-time bidi producer may only emit its next item after the peer reacts to the
current one, so peeking ahead would deadlock. Since no item can be tagged `end_of_stream=true` at
send time under this scheme, closure is signaled by a separate, payload-free frame afterward.
`ClientEngine::on_response` recognizes this frame (empty payload, no error, `end_of_stream=true`)
and closes the stream without delivering it as data, so callers never see a synthetic empty item.

**A genuine pre-existing bug surfaced and fixed during this work:** the original hand-rolled
`RpcClient` delivered stream items via `tx.try_send(...)` into a capacity-32 channel — silently
dropping data if a caller sent a burst of requests before starting to drain responses. `ClientEngine`
initially carried this pattern over faithfully; a real-server LiveKit test (64 rapid bidi messages
sent before any response is read) surfaced it with a reproducible failure rate. Fixed by making
`on_response` backpressure (`.send().await`) instead of dropping — see
`packages/tddy-rpc/src/client_engine.rs` and its regression test
`delivers_every_stream_item_even_when_the_consumer_drains_after_a_large_burst`.
