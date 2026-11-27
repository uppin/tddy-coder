# 2026-11-27 — iOS Simulator protocol, device ids, and wheel input (PR #1060)

**Type:** Feature

- `screen_sharing.proto`: `PROTOCOL_IOS_SIMULATOR = 3`, `ScreenSharingTarget.device_id = 9`,
  `AddHostTargetRequest.device_id = 10`.
- `host.proto`: `DESKTOP_CHANNEL_KIND_IOS_SIMULATOR = 3`, `HostRemoteDesktop.device_id = 11`.
- `screen_sharing_input.proto`: `ScreenSharingWheelEvent { x, y, sint32 delta_x, sint32 delta_y }`
  as `ScreenSharingInputEvent.wheel = 3`.
