# 2026-11-15 — Every ended main terminal is logged with its real outcome

**Type:** Fix

`cli_session_manager/main_exit.rs` logs one line per ended `main` — session, pid, outcome
(exit code, signal, or stopped by the daemon) and lifetime — and the recording's first write
is immediate, so a registered recording exists from registration on.

Cross-package record: [docs/dev/changesets/2026-11-15-split-agent-liveness.md](../../../../docs/dev/changesets/2026-11-15-split-agent-liveness.md).
