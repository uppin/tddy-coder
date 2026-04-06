# LiveKit Screen Capture

## Summary

A standalone CLI binary (`tddy-livekit-screen-capture`) that captures a desktop screen or application window and streams it as a LiveKit video track. The tool enumerates available capture targets (monitors and windows), connects to a LiveKit room, and publishes a continuous video stream of the selected target.

## Background

The tddy ecosystem uses LiveKit for real-time communication between the daemon, coder sessions, and web clients. Currently, LiveKit carries RPC messages over data channels and (planned) terminal output as text. Adding screen/window video streaming enables use cases like:

- Streaming a desktop environment running inside a VM or container
- Sharing an application window for remote observation
- Visual monitoring of GUI test automation

## Requirements

### Capture Targets

- **Monitor capture**: Capture an entire monitor/display
- **Window capture**: Capture a specific application window by title or ID
- **Enumeration**: `--list` flag prints all available monitors and windows with their names, dimensions, and an identifier that can be used as the capture target argument

### CLI Interface

```
tddy-livekit-screen-capture --list
tddy-livekit-screen-capture <target> [-c config.yaml] [--fps 30] [--room my-room] [--identity streamer]
```

- `<target>` — identifier from `--list` output (e.g., monitor index or window title/id)
- `-c, --config` — path to YAML config file
- `--fps` — frames per second (overrides YAML config; default: 30)
- `--room` — LiveKit room name (overrides YAML config)
- `--identity` — participant identity (overrides YAML config)
- `--list` — enumerate available capture targets and exit

### LiveKit Configuration (YAML)

Follows the same pattern as `tddy-coder` config:

```yaml
livekit:
  url: ws://127.0.0.1:7880
  api_key: devkey
  api_secret: secret
  room: screen-share
  identity: screen-capturer
  # OR use a pre-generated token:
  # token: eyJ...
```

- Supports both `api_key`/`api_secret` (generates JWT at runtime) and pre-generated `token`
- CLI flags `--room` and `--identity` override YAML values

### Video Publishing

- Publishes as `TrackSource::Screenshare`
- Uses H.264 video codec
- Track name derived from capture target (e.g., `screen-<monitor-name>` or `window-<window-title>`)
- Frame pipeline: capture (RGBA/BGRA) → I420 (YUV) → `NativeVideoSource` → LiveKit WebRTC encoding

### Platform Support

- macOS (CoreGraphics / ScreenCaptureKit via xcap)
- Linux (X11 via xcap; Wayland partial)

## Dependencies

- `xcap` — cross-platform screen and window capture (actively maintained, 588K+ downloads)
- `livekit` 0.7 / `livekit-api` 0.4 — already used in the workspace
- `serde` / `serde_yaml` — YAML config deserialization
- `clap` — CLI argument parsing
- `image` — pixel format handling (already in workspace via xcap)
- `tokio` — async runtime
- `log` / `env_logger` — logging

## Acceptance Criteria

1. `--list` enumerates all monitors and non-minimized windows with name, dimensions, and a stable identifier
2. Specifying a valid target + config starts a LiveKit room connection and publishes a video track
3. The video track is visible to other participants in the LiveKit room
4. FPS is controllable via config and CLI flag (CLI overrides config)
5. Room and identity are configurable via YAML and CLI flags
6. Token is generated from api_key/api_secret, or a pre-generated token is accepted
7. Graceful shutdown on SIGINT/SIGTERM
8. Errors (invalid target, connection failure, permission denied) produce clear messages
