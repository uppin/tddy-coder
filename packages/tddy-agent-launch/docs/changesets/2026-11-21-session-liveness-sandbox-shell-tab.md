# 2026-11-21 — A sandboxed session's shell tab runs in a jail of its own

**Type:** Feature · **Stack:** `#session-liveness` 5/8 (PR #964)

`tab_jail_launch` (the daemon's `TabJailSpawn`, lowered on the session's provider route) and the `sandbox` restore kind.

Full record: [`docs/dev/changesets/2026-11-21-session-liveness-sandbox-shell-tab.md`](../../../../docs/dev/changesets/2026-11-21-session-liveness-sandbox-shell-tab.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md#a-sandboxed-sessions-tabs).
