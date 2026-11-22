# 2026-11-21 — A dead main jail ends its terminal

**Type:** Bug Fix · **Stack:** `#tab-hardening` 3/4 (PR #991)

`jail_exit_watch` (`watch_jail_exit`, `JailProcess`, `JailBridge`, `JailEnded`, `SandboxSessionManager::subscribe_ended`); `sandbox_session.rs` split into `jail_host_bridge`, `runner_paths` and `runner_spawn` (re-exported, no path changed).

Full record: [`docs/dev/changesets/2026-11-21-jail-death-watch.md`](../../../../docs/dev/changesets/2026-11-21-jail-death-watch.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md#a-dead-main-jail).
