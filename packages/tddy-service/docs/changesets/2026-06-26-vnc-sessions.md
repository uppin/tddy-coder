# 2026-06-26 — **VNC sessions

**Type:** Feature

vnc.proto + vnc_input.proto** — new `vnc.proto`: `VncService` (6 RPCs: `ListVncTargets`, `AddVncTarget`, `RemoveVncTarget`, `UnlockVncVault`, `StartVncStream` (returns room/url/identity/track/dims), `StopVncStream`); new `vnc_input.proto`: `VncInputService.StreamInput` (bidi; `VncInputEvent` oneof pointer/key/scroll); both wired in `build.rs` with descriptor entries; `src/lib.rs` exposes `proto::vnc` and `proto::vnc_input`; TypeScript codegen regenerated. Feature [vnc-sessions.md](../../../../docs/ft/web/vnc-sessions.md). (tddy-service)
