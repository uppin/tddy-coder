# 2026-06-26 — **Screen sharing protos

**Type:** Feature

screen_sharing.proto + screen_sharing_input.proto** — new `screen_sharing.proto`: `ScreenSharingService` (6 RPCs: `ListTargets`, `AddTarget`, `RemoveTarget`, `UnlockVault`, `StartStream`, `StopStream`); `Protocol` enum (VNC=1, RDP=2); `ScreenSharingTarget` with `id`, `label`, `host`, `port`, `protocol`, `username`; `AddTargetRequest` includes `username`; new `screen_sharing_input.proto`: `ScreenSharingInputService.StreamInput` (bidi; `ScreenSharingInputEvent` oneof pointer/key); both wired in `build.rs` with descriptor entries; `src/lib.rs` exposes `proto::screen_sharing` + `proto::screen_sharing_input`; TypeScript codegen regenerated. Feature [screen-sharing-sessions.md](../../../../docs/ft/web/screen-sharing-sessions.md). (tddy-service)
