# Changesets Applied

Wrapped changeset history for tddy-rpc.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-07-01** [Feature] **Transport-agnostic RPC engines + real-time streaming fix** — `envelope` (moved from `tddy-livekit`, prost + `build.rs`), `transport::{RpcClientTransport, FrameKind, encode_frame, FrameDecoder}`, `client_engine::ClientEngine` (request-id correlation, backpressured `.send().await` stream delivery — fixes a pre-existing silent-drop-on-full-channel bug, see regression test `delivers_every_stream_item_even_when_the_consumer_drains_after_a_large_burst`), `server_engine::ServerEngine<S>` (bridge dispatch, per-`(peer, request_id)` bidi + client-streaming multiplexing, real-time item-by-item streaming forwarding with a payload-free closing signal for both server-streaming and bidi). Enables `tddy-stdio` (new stdio/IPC transport) and the `tddy-livekit` `RpcClient`/`LiveKitParticipant` refactor. Feature [rpc-multi-transport.md](../../../docs/ft/coder/rpc-multi-transport.md). (tddy-rpc)
- **2026-06-15** [Feature] **RPC Playground — `service_names()` + reflection support** — `MultiRpcService::service_names()` returns registered names in order; used by `reflection_service.rs` to scope `list_services` to only what this participant actually serves. Feature [rpc-playground.md](../../../docs/ft/daemon/rpc-playground.md). (tddy-rpc)
- **2026-03-14** [Feature] MultiRpcService — MultiRpcService and ServiceEntry for multiplexing multiple RPC services on a single participant. Enables TokenService + TerminalService on same LiveKit participant. (tddy-rpc)
- **2026-03-13** [Architecture Change] Dual-Transport Service Codegen — New package. Generic RPC framework: Status, Code, Request, Response, Streaming, RpcMessage, RpcService trait, RpcBridge, RpcResult, ResponseBody. Optional tonic feature for From conversions. (tddy-rpc)
