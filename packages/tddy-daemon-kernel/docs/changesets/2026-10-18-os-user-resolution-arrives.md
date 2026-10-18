# 2026-10-18 — `os_user_resolution` arrives from the LiveKit plugin

**Type:** Architecture (mechanical move)

`resolve_os_user` names no LiveKit crate and wraps the kernel's own `SessionUserResolver`, so the module moved
from `tddy-daemon-livekit` to `os_user_resolution.rs` here. Documented in
[daemon-kernel](../daemon-kernel.md#os_user_resolution--a-session-token-to-its-os-user).

Cross-package record: [`docs/dev/changesets/2026-10-18-satellite-5-core-modules-leave-the-plugin.md`](../../../../docs/dev/changesets/2026-10-18-satellite-5-core-modules-leave-the-plugin.md).
