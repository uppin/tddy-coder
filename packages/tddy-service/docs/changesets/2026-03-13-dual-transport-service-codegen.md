# 2026-03-13 — Dual-Transport Service Codegen

**Type:** Architecture Change

Renamed tddy-grpc to tddy-service; moved echo/terminal from tddy-livekit. Service impls (EchoServiceImpl, TerminalServiceImpl, DaemonService) in tddy-service; generated RpcService server structs + tonic adapter. (tddy-service)
