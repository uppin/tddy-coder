# Changeset: tddy-livekit-screen-capture

## Plan context (summary)

New binary package `tddy-livekit-screen-capture` captures a monitor or application window via **xcap**, converts frames to I420, and publishes **H.264** video to LiveKit as **screenshare**. CLI: `--list` enumerates targets; otherwise positional `monitor:<n>` or `window:<id>`. LiveKit settings mirror **tddy-coder** YAML (`url`, `token` or `api_key`/`api_secret`, `room`, `identity`); `--fps`, `--room`, `--identity` override YAML. Uses `tddy_livekit::TokenGenerator` for JWT when key/secret are set.

## Product documentation

- **[docs/ft/screen-capture/livekit-screen-capture.md](../../ft/screen-capture/livekit-screen-capture.md)** — feature requirements.

## Affected packages

- `packages/tddy-livekit-screen-capture` (new)
- Root `Cargo.toml` (workspace member)

## Implementation milestones

- [x] Workspace member + `Cargo.toml` with `xcap`, `livekit`, `tddy-livekit`, etc.
- [x] `config.rs` — YAML + CLI merge + token resolution
- [x] `capture.rs` — list monitors/windows, parse target, capture `RgbaImage`
- [x] `streamer.rs` — `Room::connect`, publish screenshare track, push frames (RGBA → I420)
- [x] `main.rs` — clap, loop, `tokio::signal` shutdown
- [x] Tests: config merge, target parse, CLI `--help` integration test

## Validation results

- `cargo test -p tddy-livekit-screen-capture` — pass (unit + `cli_help`).
- `cargo clippy -p tddy-livekit-screen-capture -- -D warnings` — pass.
- Root `./test` build line includes `tddy-livekit-screen-capture` so the binary is built with the standard test script.
