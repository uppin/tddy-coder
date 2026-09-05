# 2026-07-01 — **New package

**Type:** Feature

stdio/IPC RPC transport** — `StdioRpcClient` (implements `tddy_rpc::RpcClientTransport`, backed by `tddy_rpc::client_engine::ClientEngine`), `StdioBidiSender` for incremental real-time bidi sends, `StdioEndpoint` (length-prefixed framed duplex, demuxes `Request`/`Response` frames via `tddy_rpc::transport::FrameKind` so one peer can be both client and server over one pipe), `spawn_child_endpoint`/`ChildEndpoint` for parent→child process RPC. 5 acceptance tests including a reverse-call scenario (child calls a service hosted by the parent) and concurrent/streaming/bidi multiplexing over one pipe pair. Feature [rpc-multi-transport.md](../../../../docs/ft/coder/rpc-multi-transport.md). (tddy-stdio)
