# 2026-06-26 — VNC: implement ScreenSharingClient via vnc-rs

**Type:** Feature

`vnc_client.rs`: `VncClientState` implements `ScreenSharingClient` (real vnc-rs connect, RFB handshake, frame capture via `request_frame_update` + `poll_events`, pointer/key injection); `streamer.rs`: `VncStreamer` delegates to generic `Streamer<VncClientState>` from `tddy-screenshare`; `bridge.rs`: delegates to `run_bridge::<VncClientState>(config)` from `tddy-screenshare`; now depends on `tddy-screenshare`; integration test `vnc_client_integration.rs`. Feature [screen-sharing-sessions.md](../../../../docs/ft/web/screen-sharing-sessions.md). (tddy-vnc)
