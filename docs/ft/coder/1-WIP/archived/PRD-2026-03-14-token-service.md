# PRD: TokenService — LiveKit Token Generation via RPC

**Status:** ✅ Complete (documentation wrapped)
**Date:** 2026-03-14

## Summary

Expose a new `TokenService` via RPC that allows callers (web clients, CLI clients) to generate and refresh LiveKit access tokens without needing direct access to API key/secret credentials. The server-side token generation logic already exists in `tddy-livekit::TokenGenerator` — this feature wraps it as a protobuf-defined RPC service available over both gRPC and LiveKit data channels.

## Background

Currently, LiveKit token generation happens in two places:

1. **Server-side** (`tddy-livekit/src/token.rs`): `TokenGenerator` generates JWT tokens from API key/secret for the daemon's own use (connecting to rooms, auto-refresh via `run_with_reconnect`).
2. **Test infrastructure** (`tddy-livekit-testkit`, Cypress config): Tokens are generated locally in tests using hardcoded dev credentials.

In production, clients (web dashboard, remote CLI) need LiveKit tokens to join rooms and interact with the daemon. They should not hold API key/secret credentials. Instead, they should request tokens from the daemon, which already possesses the credentials.

## Proposed Changes

### New Proto Service

Define a `TokenService` in `packages/tddy-service/proto/token.proto`:

- **`GenerateToken`** — Unary RPC. Client provides room name and identity; server returns a JWT token string and its TTL.
- **`RefreshToken`** — Unary RPC. Client provides room name and identity (same or different params); server returns a fresh JWT token. Semantically identical to GenerateToken but expresses intent — the client's current token is expiring.

### Service Implementation

- `TokenServiceImpl` in `packages/tddy-service/src/token_service.rs`
- Delegates to `tddy-livekit::TokenGenerator` for actual JWT creation
- Open endpoint — no authentication required (the server already holds API key/secret; any connected client is trusted to request tokens)

### Transport Exposure

- **gRPC** (tonic): Available alongside `TddyRemote`, `TerminalService`
- **LiveKit RPC**: Available via `tddy-rpc` service generator and `RpcBridge`, following the same pattern as `EchoService` and `TerminalService`

### Daemon Integration

The daemon (tddy-coder) will register `TokenServiceServer` on both transports when LiveKit credentials (API key/secret) are configured. When only a pre-generated token is provided (no key/secret), the TokenService will not be available (no credentials to generate tokens from).

## Affected Features

- [PRD-2026-03-11-livekit-participant.md](PRD-2026-03-11-livekit-participant.md) — Adds a new service to the LiveKit participant's RPC bridge
- [grpc-remote-control.md](../../grpc-remote-control.md) — Adds a new gRPC service to the daemon

## Technical Constraints

- Reuses existing `TokenGenerator` from `tddy-livekit` — no new token generation logic
- `tddy-service` will gain a dependency on `tddy-livekit` (for `TokenGenerator`)
- Follows the established `tddy-codegen` pattern: proto → async trait + `RpcService` server + optional tonic adapter
- Token TTL is server-configured (not client-configurable) to prevent abuse

## Success Criteria

1. A `TokenService` proto definition exists with `GenerateToken` and `RefreshToken` RPCs — DONE
2. `TokenServiceImpl` delegates to `TokenProvider` (wrapping `TokenGenerator`) and returns valid JWT tokens — DONE
3. The service is accessible over LiveKit RPC — e2e test in `token_service_livekit.rs` (requires livekit feature)
4. The daemon registers the service when API key/secret are configured — DONE (via MultiRpcService with Terminal + Token)
5. `cargo test -p tddy-service` passes — DONE

Note: gRPC exposure deferred; TokenService is exposed over LiveKit RPC when the daemon uses API key/secret.

---

## Documentation wrap

Merged into [gRPC remote control](../../grpc-remote-control.md) (transport stack) and [Coder changelog](../../changelog.md) on 2026-03-21. This file is archived under `1-WIP/archived/`.
