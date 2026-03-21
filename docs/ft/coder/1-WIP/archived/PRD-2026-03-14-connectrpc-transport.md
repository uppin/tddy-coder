# PRD: ConnectRPC Transport for tddy-rpc Services

**Date**: 2026-03-14
**Status**: ✅ Complete (documentation wrapped)
**Affected features**: [grpc-remote-control](../../grpc-remote-control.md)

## Summary

Add a ConnectRPC HTTP transport (`tddy-connectrpc`) that exposes `tddy-rpc`-based services over the Connect protocol. Any service implementing `RpcService` (e.g. `EchoService`, `TerminalService`) can be served over ConnectRPC — the same services already exposed via gRPC (tonic) and LiveKit data channel.

The transport mounts at `/rpc` on tddy-coder's existing web-port (axum), enabling standard HTTP clients to call services without gRPC or LiveKit dependencies.

## Background

Today, `tddy-rpc` services are exposed through two transports:
1. **gRPC** (tonic) — for programmatic control (tddy-service)
2. **LiveKit data channel** — for browser-based real-time communication (tddy-livekit)

Both require specialized clients. ConnectRPC enables plain HTTP access using the Connect protocol, which is:
- Curl-friendly (JSON bodies, standard HTTP)
- Browser-compatible (no gRPC-web proxy needed)
- Protobuf-compatible (binary encoding for performance)

## Proposed Changes

### New crate: `packages/tddy-connectrpc`

An axum-based HTTP handler that implements the Connect protocol:
- **URL pattern**: `POST /rpc/<package>.<Service>/<Method>`
- **Content-Type negotiation**: supports both `application/json` and `application/proto`
- **Error format**: Connect protocol error JSON (`{ code, message, details }`)
- **Required header**: `Connect-Protocol-Version: 1`
- **All four RPC patterns**: unary, server streaming, client streaming, bidi streaming
- **Logging**: structured request/response logging via `log` crate, compatible with existing `rpc_trace!` macro

### Integration with tddy-coder

- Mount ConnectRPC router at `/rpc` on the existing web-port
- Reuse existing `RpcService` implementations (no service changes needed)
- Web server serves both static files and ConnectRPC endpoints

### Integration test package: `packages/tddy-rust-typescript-tests`

- Bun-based test package using `bun:test`
- Tests run a Rust-built echo server (the same `EchoServiceImpl`) over ConnectRPC
- TypeScript client uses `@connectrpc/connect` + `@connectrpc/connect-node`
- Validates all four RPC patterns end-to-end
- Structured logging on both server and client for development visibility

## Impact

### Technical
- New crate with axum dependency (already in workspace)
- New bun workspace member for integration tests
- Minimal changes to tddy-coder (mount router)
- No changes to existing services or transports

### User
- Services become accessible via standard HTTP
- Curl-testable endpoints for debugging
- Web clients can call services directly without LiveKit

## Success Criteria

1. `EchoService.Echo` callable via `curl` with JSON body
2. All four RPC patterns work through ConnectRPC transport
3. TypeScript integration tests pass with `bun test`
4. Content-Type negotiation works (JSON and protobuf binary)
5. Error responses follow Connect protocol format
6. Logging provides clear visibility into request/response lifecycle
7. ConnectRPC router mounts cleanly alongside static file serving on web-port

---

## Documentation wrap

Merged into [gRPC remote control](../../grpc-remote-control.md) (transport stack) and [Coder changelog](../../changelog.md) on 2026-03-21. This file is archived under `1-WIP/archived/`.
