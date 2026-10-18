# 2026-10-18 — `placement` and `session_git` arrive from the LiveKit plugin

**Type:** Architecture (mechanical move)

Both modules name no LiveKit crate and have one consumer here, so they moved from `tddy-daemon-livekit`
into this crate: `placement` (`classify_placement`) and `session_git` (`session_git_environment`), with the
`session_git` suite at `tests/session_git_identity.rs`. The instance-id imports in this crate come from
`tddy_daemon_kernel::daemon_identity`. Layout: [module-layout](../module-layout.md#placement-and-the-sessions-git-environment).

Cross-package record: [`docs/dev/changesets/2026-10-18-satellite-5-core-modules-leave-the-plugin.md`](../../../../docs/dev/changesets/2026-10-18-satellite-5-core-modules-leave-the-plugin.md).
New code issue (alert-only, not grown): `oversized-file-session-coordinate-handlers`, 503 production lines.
