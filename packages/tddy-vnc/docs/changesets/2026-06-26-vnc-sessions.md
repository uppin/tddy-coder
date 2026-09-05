# 2026-06-26 — **VNC sessions

**Type:** Feature

tddy-vnc package scaffold (bridge stubs)** — new package: `vnc_client.rs` (VncClientState stub), `streamer.rs` (VncStreamer stub), `bridge.rs` (run pump loop stub), `common.rs` (char_to_keysym, rgba_to_abgr implemented + unit tests), `main.rs` (reads JSON BridgeConfig from stdin, calls bridge::run); deps: tddy-livekit, livekit 0.7, vnc-rs, image, prost, tokio, anyhow; vnc_client/streamer/bridge are follow-up implementation stubs (FIXMEs). Feature [vnc-sessions.md](../../../../docs/ft/web/vnc-sessions.md). (tddy-vnc)
