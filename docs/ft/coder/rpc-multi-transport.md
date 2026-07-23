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

## Remaining IPC migration (in progress, source: `docs/dev/TODO.md`)

The transport itself is done and proven, but three local IPC use-cases that predate it still run
over gRPC or a bespoke protocol instead of adopting it. This is not a transport gap — it's call
sites that haven't been switched over yet. gRPC is not being removed globally (the `--grpc`
remote-control surface, the `--daemon` gRPC server, and the sandbox-runner's own gRPC server for
`tddy-sandbox-app` are unaffected); only gRPC/bespoke-JSON used purely as *local* IPC is in scope,
and per this repo's convention there is no dual-path fallback — each call site's old transport is
deleted once it moves to stdio.

1. **Daemon ↔ sandbox-runner session control channel.** `tddy-daemon`'s real session lifecycle
   (`connection_service.rs` spawn/dial orchestration, `sandbox_session.rs::dial_and_bridge`) still
   spawns `tddy-sandbox-runner` with `--grpc-socket`/`--grpc-uds`/`--grpc-listen-port` and dials the
   tonic `SandboxServiceClient`, for every real sandboxed session. The stdio primitives it needs
   (`bridge_sandbox_stdio`, `StdioSandboxClient`, the transport-agnostic `run_host_relay`) are
   already built and proven end-to-end through a real Seatbelt jail — only the daemon's own
   spawn/dial call sites remain to be switched. Once done, the gRPC spawn flags and their
   supporting port/handshake code are deleted outright (no dual-path fallback).
2. **Linux (`tddy-sandbox-cgroups`) jail-spawn stdio piping.** `tddy-sandbox-darwin::spawn_plan`
   pipes stdin/stdout when `--stdio` is present in the runner's argv; the Linux cgroups+namespaces
   jail-spawn path needs the equivalent change before (1) can work cross-platform.
3. **Toolcall listener.** `tddy-core/src/toolcall/listener.rs` is a third, unrelated bespoke
   newline-delimited-JSON protocol (`submit`/`ask`/`approve`/`list-actions`/`invoke-action`/
   `build`/`build-list`) between `tddy-coder` and the Claude Code CLI subprocess it spawns — same
   category of problem (bespoke local IPC where `tddy-rpc`/`tddy-stdio` already fits), same fix.

See the `finish-stdio-ipc-migration` changeset for acceptance criteria and test coverage per item.

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

### LiveKit oversized-frame chunking

The LiveKit data-channel transport negotiates an SCTP max message size of 64,000 bytes; a
larger `publish_data` is rejected outright, and `SharedPublisher` retries the same doomed
publish 30× forever — wedging the transport and starving small RPCs like `ListSessions` (the
web dashboard then sees `premature EOF` / `cant skip wire type 4` protobuf decode errors).
This surfaced in production when the daemon published oversized `StreamSessionActivity`
snapshots (a 369 KB tool-result record, plus 71/117/187 KB others).

The LiveKit transport now chunks oversized envelopes. `chunking::frame_for_transport` splits a
payload into ≤60,000-byte frames, each carrying a 13-byte little-endian header
`[magic: 0x00][message_id: u32][total_chunks: u32][index: u32]`, and each receive loop
reassembles per sender via a `ChunkReassembler` — message ids are unique only within one sender
(process-wide `next_message_id` counter), and a browser talks to several daemons on one room, so
mixing two senders' chunks would corrupt reassembly. Reassembly is index-keyed, so frames need
not arrive in order. This is LiveKit-specific: stdio's length-prefixed frame codec has no such
size limit.

Back-compat is preserved by the magic byte. A payload that fits one packet is sent **raw**
(byte-for-byte the encoded envelope), so a pre-chunking peer still decodes small messages; only
an oversized payload is split. A valid prost `RpcRequest`/`RpcResponse` can never begin with
`0x00` (protobuf field number 0 is illegal, and the envelopes' lowest field numbers are 1/2), so
the receiver disambiguates raw vs chunk on the first byte via `is_chunk_frame`. Send paths
(`client.rs::publish_request`, both `participant.rs` response drains) frame through
`frame_for_transport`; receive paths (`client.rs`, `client_factory.rs`, `participant.rs`)
reassemble per sender. The TypeScript client mirrors the codec byte-for-byte in
`packages/tddy-livekit-web/src/chunking.ts`, wired into `transport.ts`
(`publishRequest` + per-sender reassembly in `RoomRpcRegistry` and the standalone
`LiveKitTransport`). Verified by a real-LiveKit `rpc_scenarios` test that round-trips a 370 KB
oversized echo in both directions.

*Follow-up (deferred, separately tracked):* cap `result_json`/`input_json` at the activity-log
layer (`tddy-daemon/src/tool_call_log.rs`) so individual records stay small regardless of
transport; a Cypress e2e crossing the TS↔Rust boundary with an oversized payload is also
outstanding.
