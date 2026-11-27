# 2026-11-27 — ConnectTarget, wheel input, and the RGBA→I420 byte order (PR #1060)

**Type:** Feature / Fix

- `ScreenSharingClient::connect` and `inspect` take a `ConnectTarget { host, port, device }`
  (`ConnectTarget::network` for VNC/RDP); `BridgeConfig.device` and `connect_target()` carry an iOS
  Simulator udid from the daemon.
- `inject_wheel(x, y, delta_x, delta_y)` on the trait; `stream_input` maps
  `ScreenSharingWheelEvent` to it.
- **Fix:** frames were reordered RGBA → ABGR bytes before libyuv's `abgr_to_i420`, which already
  reads R, G, B, A in memory, so alpha was read as red and colours washed towards pink.
  `rgba_to_i420` passes RGBA as it is (`tests/rgba_to_i420.rs`).

See [client-and-bridge.md](../client-and-bridge.md).
