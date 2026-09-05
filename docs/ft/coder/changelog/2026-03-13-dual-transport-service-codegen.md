# 2026-03-13 — Dual-Transport Service Codegen

- **tddy-rpc**: New package. Generic RPC framework: Status, Code, Request, Response, Streaming, RpcMessage, RpcService trait, RpcBridge, RpcResult, ResponseBody. Optional tonic feature.
- **tddy-codegen**: Renamed from tddy-livekit-codegen. TddyServiceGenerator generates transport-agnostic service traits, RpcService server structs (per-method handlers, service name validation), tonic adapters (feature-gated).
- **tddy-service**: Renamed from tddy-grpc. Service impls (EchoServiceImpl, TerminalServiceImpl, DaemonService) live here; no transport dependencies.
- **tddy-livekit**: Slimmed to thin LiveKit adapter. Proto envelope, participant, RpcRequest→RpcMessage→RpcBridge. Depends on tddy-rpc only; no service impls.
- **Application layer**: Glues tddy-service + tddy-livekit at runtime (EchoServiceServer + LiveKitParticipant).
