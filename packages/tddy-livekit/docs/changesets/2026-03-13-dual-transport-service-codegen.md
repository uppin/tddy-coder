# 2026-03-13 — Dual-Transport Service Codegen

**Type:** Architecture Change

Slimmed to thin LiveKit transport adapter. Proto envelope (rpc_envelope.proto), participant, RpcRequest→RpcMessage→RpcBridge. Depends on tddy-rpc only; no service impls. (tddy-livekit)
