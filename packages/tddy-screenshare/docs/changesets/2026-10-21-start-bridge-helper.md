# 2026-10-21 — `start_bridge` publishes through `publish_to_livekit`

**Type:** Refactor

`#host-desktops` 10/10 ([#693](https://github.com/uppin/tddy-coder/pull/693)). No behaviour change. The engine
extracted the LiveKit join, the track publish and the RPC-loop spawn out of `start_bridge` as
`publish_to_livekit(config, width, height) -> (input receiver, streamer)`: `start_bridge` 58 ➜ **31** lines.
The assist copied `start_bridge`'s unused `<C: ScreenSharingClient>` onto the helper (E0283, then
`clippy::extra_unused_type_parameters`); it was deleted by hand (a recorded engine defect). `bridge.rs` 399 ➜ 404
production lines. Baseline: 9 passed / 0 failed before and after.
