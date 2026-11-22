# 2026-11-22 — Stopping a Coder-target session reads as `exit code 1`

**Category:** Future enhancement (found by `fix/coder-target-resume`, #998)
**Source:** `packages/tddy-cli-sessions/src/cli_session_manager/main_exit.rs` (`log_main_exit`), the local `coder ssh` process a target session's main runs

An operator's Stop of a session whose agent runs on a Coder workspace sends SIGTERM to the session's
local process, which is `coder ssh`, not the agent. `coder ssh` handles the signal and exits with
status 1, so the daemon logs `main exited: … outcome=exit code 1` at `warn` — reading as a failure —
where a host session reads as stopped by a signal.

**Why deferred:** it did not affect resume (#998 found the resume went ahead), and telling a deliberate
stop from a real failure needs the main-exit classification to know the daemon sent the signal.

**What would close it:** classify a main that exits after the daemon signalled it as "stopped by the
daemon" regardless of the exit status its transport reports, and cover a target stop in
`coder_target_session_acceptance.rs`.
