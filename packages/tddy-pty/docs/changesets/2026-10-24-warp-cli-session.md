# 2026-10-24 — Exit reported once the terminal's output is drained

**Type:** Fix

`#warp-agent` 1/2, PR [#717](https://github.com/uppin/tddy-coder/pull/717). Cross-package entry:
[2026-10-24-warp-cli-session.md](../../../../docs/dev/changesets/2026-10-24-warp-cli-session.md).

- `runtime.rs`: the exit monitor waits for the PTY reader to drain before reporting exit, so the capture
  holds the process's last output (Warp's exit banner) when the exit is seen. The wait is bounded by
  `READER_DRAIN_GRACE` (1 s), so a process left holding the terminal — possible on Linux; macOS revokes
  the terminal when the session leader exits — cannot hold the exit back.
- The PTY control is registered in the PTY registry before the ready signal, and removed the moment the
  process exits, so its absence means the process has exited (`tddy-cli-sessions`' `exit_hook_settled`
  relies on it).

Tests (runtime unit tests): `output_written_just_after_the_child_exits_is_captured_when_the_exit_is_reported`
(Linux only), `a_process_left_holding_the_terminal_does_not_hold_back_the_exit`.
