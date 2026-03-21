# PRD: Dual-Transport Service Codegen (`tddy-codegen`)

**Status:** ✅ Complete (documentation wrapped)
**Date:** 2026-03-13

## Summary

Create `tddy-rpc` as the generic RPC framework (types + dispatch), rebrand `tddy-livekit-codegen` to `tddy-codegen`, rename `tddy-grpc` to `tddy-service`, slim `tddy-livekit` to a thin LiveKit transport adapter, and enhance codegen to generate transport-agnostic service traits + RpcService server structs + tonic adapter structs. Service implementations live in `tddy-service`; transports (`tddy-livekit`, tonic) are glued at the application layer.

## Architecture

### `tddy-rpc` (NEW)
Generic RPC framework — protocol-agnostic:
- Types: `Status`, `Code`, `Request<T>`, `Response<T>`, `Streaming<T>`, `RpcMessage`, `RequestMetadata`
- Dispatch: `RpcService` trait, `RpcBridge`, `RpcResult`, `ResponseBody`
- Optional `tonic` feature for From conversions

### `tddy-service` (RENAMED from `tddy-grpc`)
Transport-agnostic service definitions:
- Proto files: echo, terminal, remote
- Generated: traits, server structs (`EchoServiceServer<T>`), tonic adapters (feature-gated)
- Service impls: `EchoServiceImpl`, `TerminalServiceImpl`, `DaemonService`
- Depends on `tddy-rpc` only

### `tddy-livekit`
Thin LiveKit transport adapter:
- Proto envelope (`rpc_envelope.proto`)
- Participant: room, data channel, stream accumulation
- Converts `RpcRequest` -> `RpcMessage` -> `RpcBridge`
- Depends on `tddy-rpc` only — NOT `tddy-service`

### Application layer
Glues `tddy-service` + `tddy-livekit` at runtime:
```rust
let server = EchoServiceServer::new(EchoServiceImpl::new());
let participant = LiveKitParticipant::connect(url, token, server, opts).await?;
```

## Success Criteria

1. `tddy-rpc` contains all generic RPC types and dispatch — no protocol-specific dependencies
2. `tddy-service` contains all service definitions and impls — no transport dependencies
3. `tddy-livekit` is a thin LiveKit adapter — no generic RPC logic, no service implementations, no dependency on `tddy-service`
4. `EchoServiceImpl` (in `tddy-service`) has zero hand-written `RpcService` routing boilerplate
5. `TerminalServiceImpl` (in `tddy-service`) has zero hand-written `RpcService` routing boilerplate
6. All existing integration tests pass (wired via `tddy-service` as dev-dependency)
7. `cargo test -p tddy-livekit -p tddy-service -p tddy-codegen -p tddy-rpc` all pass

---

## Documentation wrap

Merged into [gRPC remote control](../../grpc-remote-control.md) (transport stack) and [Coder changelog](../../changelog.md) on 2026-03-21. This file is archived under `1-WIP/archived/`.
