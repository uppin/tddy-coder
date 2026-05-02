# tddy-livekit-screen-capture

CLI binary that captures a **monitor** or **application window** (via [xcap](https://crates.io/crates/xcap)) and publishes **H.264** video to a LiveKit room as **screenshare**.

## Usage

```bash
# List monitors and windows (ids for TARGET)
tddy-livekit-screen-capture --list

# Stream (requires Screen Recording permission on macOS)
tddy-livekit-screen-capture monitor:0 -c screen-capture.example.yaml
tddy-livekit-screen-capture window:12345 -c screen-capture.example.yaml --fps 60 --room my-room --identity me
```

LiveKit settings use the same YAML shape as **tddy-coder** (`livekit.url`, `token` or `api_key`/`api_secret`, `room`, `identity`). Optional top-level `fps` defaults to 30; `--fps`, `--room`, and `--identity` override the file.

See [screen-capture.example.yaml](./screen-capture.example.yaml) in this directory.

## Workspace

- Crate: `packages/tddy-livekit-screen-capture`
- Binary: `tddy-livekit-screen-capture`

## Documentation

- Product requirements: [docs/ft/screen-capture/livekit-screen-capture.md](../../docs/ft/screen-capture/livekit-screen-capture.md)
- [Changesets](./docs/changesets.md)
