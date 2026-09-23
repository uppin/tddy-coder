# 2026-09-23 — A session room's service list is built only once a room will open

**Type:** Fix

`#carve` 11/12 ([#520](https://github.com/uppin/tddy-coder/pull/520)). Cross-package entry: [2026-09-23-carve-rpc-handlers.md](../../../../docs/dev/changesets/2026-09-23-carve-rpc-handlers.md).

`SessionRoomRegistry::{ensure_open, open_measured_by}` take the room's RPC surface as
`impl FnOnce() -> Result<S, Status>` and call it after the LiveKit-credentials check. A daemon with no
credentials returns `Ok(None)` without building it, so the session host's roster — which refuses with
`FAILED_PRECONDITION` when its RPC families were never wired — can no longer turn "no room" into a
refused `ConnectSession` (validation finding W1). `open` wraps a built service.
