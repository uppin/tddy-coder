# Changesets Applied

Wrapped changeset history for tddy-vnc.

**Merge hygiene:** [Changelog merge hygiene](../../../docs/dev/guides/changelog-merge-hygiene.md) — prepend one single-line bullet; do not rewrite shipped lines.

- **2026-06-26** [Feature] **VNC sessions — tddy-vnc package scaffold (bridge stubs)** — new package: `vnc_client.rs` (VncClientState stub), `streamer.rs` (VncStreamer stub), `bridge.rs` (run pump loop stub), `common.rs` (char_to_keysym, rgba_to_abgr implemented + unit tests), `main.rs` (reads JSON BridgeConfig from stdin, calls bridge::run); deps: tddy-livekit, livekit 0.7, vnc-rs, image, prost, tokio, anyhow; vnc_client/streamer/bridge are follow-up implementation stubs (FIXMEs). Feature [vnc-sessions.md](../../../docs/ft/web/vnc-sessions.md). (tddy-vnc)
