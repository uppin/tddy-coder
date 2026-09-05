# 2026-07-01 — Transport-agnostic RPC engines + real-time streaming fix

**Type:** Feature

`envelope` (moved from `tddy-livekit`, prost + `build.rs`), `transport::{RpcClientTransport, FrameKind, encode_frame, FrameDecoder}`, `client_engine::ClientEngine` (request-id correlation, backpressured `.send().await` stream delivery — fixes a pre-existing silent-drop-on-full-channel bug, see regression test `delivers_every_stream_item_even_when_the_consumer_drains_after_a_large_burst`), `server_engine::ServerEngine<S>` (bridge dispatch, per-`(peer, request_id)` bidi + client-streaming multiplexing, real-time item-by-item streaming forwarding with a payload-free closing signal for both server-streaming and bidi). Enables `tddy-stdio` (new stdio/IPC transport) and the `tddy-livekit` `RpcClient`/`LiveKitParticipant` refactor. Feature [rpc-multi-transport.md](../../../../docs/ft/coder/rpc-multi-transport.md). (tddy-rpc)
