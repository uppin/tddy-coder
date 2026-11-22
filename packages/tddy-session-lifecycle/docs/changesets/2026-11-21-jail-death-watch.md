# 2026-11-21 — A dead main jail ends its terminal

**Type:** Bug Fix · **Stack:** `#tab-hardening` 3/4 (PR #991)

`SandboxTerminalSession::subscribe_pty_done` follows the jail's recorded exit; the parity suite's stand-in jail is a running `sleep`.

Full record: [`docs/dev/changesets/2026-11-21-jail-death-watch.md`](../../../../docs/dev/changesets/2026-11-21-jail-death-watch.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md#a-dead-main-jail).
