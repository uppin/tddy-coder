# 2026-07-01 — RPC multi-transport: stdio/IPC alongside LiveKit

- New `tddy-stdio` package: parent↔child process RPC over stdin/stdout, multiplexed, callable in either direction (the child can call back into the parent over the same pipe pair)
- `tddy-rpc` gained shared transport-agnostic engines (`ClientEngine`, `ServerEngine`) extracted from what was previously LiveKit-specific dispatch logic
- `tddy-livekit`'s `RpcClient` now implements the same `RpcClientTransport` trait as the new stdio client — the same calling code works over either transport
- Server-streaming and bidi RPC responses are now genuinely real-time (previously server-streaming held back each response by one item while waiting to see if another was coming)
- Fixed a data-loss bug where a burst of RPC calls sent before reading any responses could silently drop responses once an internal buffer filled
- Feature: [rpc-multi-transport.md](../rpc-multi-transport.md)
