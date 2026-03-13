# PRD: LiveKit ConnectRPC Transport for Browser (TypeScript)

**Status:** WIP
**Date:** 2026-03-13

## Summary

Add a browser-based TypeScript package (`packages/tddy-livekit-web`) that implements a ConnectRPC `Transport` over LiveKit data channels. This enables any browser-based ConnectRPC client to call unary and streaming RPCs served by a Rust `LiveKitParticipant` — using the same protobuf envelope protocol (`RpcRequest`/`RpcResponse`) already defined in `packages/tddy-livekit`.

All four streaming types must be supported: unary, client streaming, server streaming, and bidirectional streaming.

## Background

The Rust `tddy-livekit` package already implements:

- **Envelope protocol**: `RpcRequest` / `RpcResponse` protobuf messages over LiveKit data channel (topic `tddy-rpc`)
- **Server participant**: `LiveKitParticipant` receives RPC requests and dispatches to `RpcBridge` → `RpcService`
- **Rust RPC client**: `RpcClient` sends requests and correlates responses via `request_id`
- **Echo service**: Test service with `Echo` (unary) and `EchoServerStream` (server streaming) methods

A reference implementation (`wasm-proto-transport`) exists for a similar ConnectRPC custom transport over WASM FFI boundaries. The key difference: LiveKit data channels are inherently async (push-based events), so no polling is needed — responses arrive via `dataReceived` events.

### Why ConnectRPC Transport?

ConnectRPC's `Transport` interface is transport-agnostic, requiring only `unary()` and `stream()` methods. By implementing a custom transport that serializes/deserializes the RPC envelope and sends/receives via LiveKit data channels, any TypeScript service client generated from `.proto` files can transparently call Rust services through LiveKit rooms.

## Proposed Changes

### New Package: `packages/tddy-livekit-web`

A TypeScript library implementing:

1. **`LiveKitTransport`** — ConnectRPC `Transport` implementation
   - `unary()`: serialize `RpcRequest`, publish via LiveKit, await `RpcResponse` correlated by `request_id`
   - `stream()`: handle all streaming patterns (client, server, bidi) via `AsyncQueue` and `request_id` correlation
   - Uses `@livekit/client-sdk-js` for room connection and data channel I/O
   - Uses `@bufbuild/protobuf` for protobuf serialization
   - Uses `@connectrpc/connect` for `Transport` interface

2. **`AsyncQueue<T>`** — backpressure-aware async channel for streaming responses
   - `enqueue(item)` / `dequeue()` / `close()` / `fail(error)`
   - Implements `AsyncIterableIterator` for use in `StreamResponse.message`

3. **Proto codegen** — TypeScript bindings for the RPC envelope and echo service
   - `@bufbuild/protoc-gen-es` to generate from `proto/rpc_envelope.proto` and `proto/test/echo_service.proto`

4. **Cypress component tests** — validate all RPC patterns against a real Rust echo server

### Rust Changes (packages/tddy-livekit)

The existing Rust server supports unary + server streaming. To support all four streaming types:

1. **Proto changes**: Add `EchoClientStream` and `EchoBidiStream` methods to `echo_service.proto`
2. **Bridge/Participant changes**: Handle multi-message client streams (accumulate messages by `request_id` until `end_of_stream=true`)
3. **Echo service implementation**: Implement the new client streaming and bidi streaming echo methods
4. **Integration tests**: Add Rust-side tests for client streaming and bidi streaming

### Test Infrastructure

A Cypress plugin/task that:
1. Starts (or reuses) a LiveKit server via `LIVEKIT_TESTKIT_WS_URL`
2. Starts a Rust echo server participant in the LiveKit room
3. Generates LiveKit access tokens for TS test clients
4. Cleans up on test completion

## Affected Features

- [PRD-2026-03-11-livekit-participant.md](PRD-2026-03-11-livekit-participant.md) — Extends with client streaming and bidi streaming support

## Architecture

```
TypeScript Client (Browser)
    │
    ▼
ConnectRPC createClient(EchoService, transport)
    │
    ▼
LiveKitTransport (implements Transport)
    │  serialize RpcRequest (protobuf)
    │  room.localParticipant.publishData()
    │  topic: "tddy-rpc"
    ▼
LiveKit Data Channel ──── network ────►
    │                                   Rust LiveKitParticipant
    │                                   ► RpcBridge ► EchoService
    │                                          │
    ◄── LiveKit Data Channel ◄── RpcResponse ◄─┘
    │
    │  room.on('dataReceived')
    │  correlate response.request_id → pending request
    │  deserialize RpcResponse
    ▼
UnaryResponse / StreamResponse
```

### Key difference from WASM transport

LiveKit data channels are async and push-based. Each `RpcResponse` arrives as a separate data packet event. This eliminates the need for:
- Polling mechanisms
- Batch response decoding
- Length-delimited message framing

Each response message is a standalone LiveKit data packet containing one `RpcResponse` protobuf.

## Streaming Patterns

### Unary
1. TS sends `RpcRequest` with `end_of_stream=true`
2. Rust responds with `RpcResponse` containing `response_message` + `end_of_stream=true`

### Client Streaming
1. TS sends multiple `RpcRequest` messages with same `request_id`, `end_of_stream=false`
2. Final message has `end_of_stream=true`
3. Rust accumulates messages, processes when stream ends
4. Rust sends single `RpcResponse` with `end_of_stream=true`

### Server Streaming
1. TS sends `RpcRequest` with the request payload
2. Rust sends multiple `RpcResponse` messages with same `request_id`
3. Final response has `end_of_stream=true`
4. TS collects responses into `AsyncQueue`, caller iterates via `for await`

### Bidirectional Streaming
1. TS sends multiple `RpcRequest` messages
2. Rust sends multiple `RpcResponse` messages concurrently
3. Both sides use `end_of_stream=true` to signal completion
4. TS uses `AsyncQueue` for received messages

## Dependencies (new)

| Package | Purpose |
|---------|---------|
| `@livekit/lk-server-sdk` | LiveKit server SDK (token generation for tests) |
| `livekit-client` | LiveKit client SDK (room connection, data channels) |
| `@bufbuild/protobuf` | Protobuf runtime (create, toBinary, fromBinary) |
| `@connectrpc/connect` | Transport interface, createClient |
| `@bufbuild/buf` | (dev) Proto codegen CLI |
| `@bufbuild/protoc-gen-es` | (dev) TypeScript proto codegen plugin |

## Success Criteria

1. A TypeScript ConnectRPC client can call `Echo` (unary) on a Rust echo server via LiveKit data channel
2. Server streaming works: client receives multiple messages from `EchoServerStream`
3. Client streaming works: client sends multiple messages, receives aggregated response
4. Bidirectional streaming works: both sides send/receive messages concurrently
5. Cypress component tests pass against a real LiveKit server + Rust echo server
6. The transport correctly handles errors (unknown service, unknown method, error responses)
7. AbortSignal support for cancellation
8. `bun run cypress:component` in the new package passes all tests
