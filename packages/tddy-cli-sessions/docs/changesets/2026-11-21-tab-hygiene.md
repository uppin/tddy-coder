# 2026-11-21 — Tab hygiene

**Type:** Bug Fix · **Stack:** `#tab-hardening` 2/4 (PR #990)

`process_start` (`ProcessStart`, `process_start(pid)` through `sysinfo`), `pane_pid_and_start`, and `kill_tmux_session_logged` (replacing two private copies).

Full record: [`docs/dev/changesets/2026-11-21-tab-hygiene.md`](../../../../docs/dev/changesets/2026-11-21-tab-hygiene.md); feature: [`session-liveness.md`](../../../../docs/ft/daemon/session-liveness.md#deleting-a-tmux-run-session).
