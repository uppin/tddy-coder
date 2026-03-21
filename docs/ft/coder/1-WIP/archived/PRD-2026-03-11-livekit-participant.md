# PRD: LiveKit Room Participant for tddy-coder Daemon

**Status:** ✅ Complete (documentation wrapped)
**Date:** 2026-03-11

## Summary

Enable the tddy-coder daemon to connect to a LiveKit room as a participant, exposing RPC services over LiveKit data channels using a custom protobuf-based transport layer. This extends the daemon's reach beyond local gRPC — any LiveKit room participant can invoke TddyRemote operations and new LiveKit-specific services through typed, streaming-capable protobuf RPC.

## Background

The daemon currently exposes its functionality via a tonic gRPC server (local TCP). This limits access to clients on the same network. By joining a LiveKit room, the daemon becomes reachable from any participant in that room — browsers, mobile apps, other servers — without requiring direct network connectivity.

LiveKit data channels provide reliable (ordered, guaranteed delivery) and lossy (unordered, best-effort) transport between room participants. Building a custom protobuf RPC layer on top of this transport enables:

- Typed service interfaces with full protobuf schema
- Streaming support (server, client, and bidirectional)
- Service routing via an envelope protocol
- Reuse of existing TddyRemote service definitions

### Why custom RPC instead of LiveKit's built-in RPC?

LiveKit's built-in `register_rpc_method` / `perform_rpc` is string-based and unary-only. The daemon needs:

1. **Typed protobuf messages** — schema-driven, generated code, backward compatibility
2. **Streaming** — server-streaming for event subscriptions, bidirectional for interactive workflows
3. **Service multiplexing** — multiple services routed through a single data channel

The reference implementation at `scenic-engine/packages/wasm-proto-transport` demonstrates this pattern over a WASM boundary. This feature adapts the same envelope protocol for LiveKit data channels.

## Proposed Changes

### New Packages

#### 1. `tddy-livekit` — LiveKit RPC transport + room participant

- **Envelope protocol**: `RpcRequest` / `RpcResponse` protobuf messages for multiplexing service/method calls over data channels
- **Custom prost `ServiceGenerator`**: Generates Rust service traits, client stubs, and handler dispatch from `.proto` service definitions (analogous to `WasmServiceGenerator` in wasm-proto-transport, but targeting LiveKit data channels)
- **`RpcService` trait**: Service routing and dispatch; implementations handle incoming requests and produce responses
- **`RpcBridge`**: Manages active requests, streaming state, and message routing between LiveKit data channel and service implementations
- **LiveKit room participant**: Connects to a LiveKit room, subscribes to data channel messages, and feeds them into the RpcBridge
- **RPC client**: Sends typed protobuf requests over data channel to a remote participant's service

Data channels used:
- **Reliable** (TCP-like): All RPC traffic — request/response envelopes
- **Lossy** (UDP-like): Reserved for future real-time streaming use cases

#### 2. `tddy-livekit-testkit` — LiveKit testcontainers

- Uses `testcontainers` Rust crate to manage a LiveKit server Docker container
- Docker image: `livekit/livekit-server` (`:master` or pinned version)
- Dev mode with built-in API key/secret (`devkey` / `secret`)
- Token generation via `livekit-api` crate
- Provides helper to get WebSocket URL and generate participant tokens
- Pattern: same as `makers-lt/packages/livekit-testkit/` but in Rust

### Daemon Integration

The daemon connects to a LiveKit room on startup when configured via CLI arguments:

- `--livekit-url <URL>` — LiveKit server WebSocket URL (e.g. `ws://localhost:7880`)
- `--livekit-token <TOKEN>` — Access token for room join
- `--livekit-room <ROOM>` — Room name
- `--livekit-identity <IDENTITY>` — Participant identity

When these flags are provided, the daemon:
1. Connects to the LiveKit room as a participant
2. Registers an `RpcBridge` on the data channel
3. Serves incoming RPC requests from other room participants
4. Continues running the existing gRPC server in parallel

### Services Exposed

1. **Existing TddyRemote operations** — GetSession, ListSessions, Stream — accessible over LiveKit data channel
2. **New LiveKit-specific services** — to be defined as needs emerge (e.g. room presence, participant status)

## Affected Features

- [grpc-remote-control.md](../../grpc-remote-control.md) — Daemon startup flow changes (additional LiveKit connection alongside gRPC server)

## Technical Constraints

- The LiveKit Rust SDK (`livekit` crate) handles room connection and data channel I/O
- `prost` + `prost-build` for protobuf compilation; custom `ServiceGenerator` for trait generation
- `testcontainers` crate for integration test infrastructure
- Docker must be available for running LiveKit testcontainer tests
- Reliable data channel has a 15 KiB per-packet limit; the envelope protocol must handle message framing

## Dependencies (new external crates)

| Crate | Version | Purpose |
|-------|---------|---------|
| `livekit` | 0.7 | LiveKit Rust SDK (room, data channels) |
| `livekit-api` | 0.4 | Token generation, server-side API |
| `testcontainers` | 0.27 | Docker container management for tests |
| `prost` | (existing) | Protobuf runtime |
| `prost-build` | (existing) | Protobuf code generation |

## Success Criteria

1. A `tddy-coder --daemon --livekit-url ws://... --livekit-token ... --livekit-room test` process joins the specified LiveKit room as a participant
2. Another participant in the same room can invoke RPC methods (e.g. `ListSessions`) via the custom protobuf transport over the data channel
3. Streaming RPCs work — a client can subscribe to session events via server-streaming RPC
4. Integration tests using testcontainers spin up a real LiveKit server, connect two participants, and verify end-to-end RPC calls
5. The custom `ServiceGenerator` produces correct service traits and handlers from `.proto` definitions
6. `cargo test -p tddy-livekit` and `cargo test -p tddy-livekit-testkit` pass

---

## Documentation wrap

Merged into [gRPC remote control](../../grpc-remote-control.md) (transport stack) and [Coder changelog](../../changelog.md) on 2026-03-21. This file is archived under `1-WIP/archived/`.
