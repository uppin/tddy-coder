# 2026-10-18 — Five Rust tests flaked once each in CI and have not been looked at

**Category:** Flaky tests (watch list)
**Source:** four CI attempts of [#651](https://github.com/uppin/tddy-coder/pull/651), plus the CI run of
[#645](https://github.com/uppin/tddy-coder/pull/645)

The `ci` nextest profile retries twice, so each of these ended green: `FLAKY 2/3`, passing on the second try. The
profile reports them in its final status line, which is the only place they show.

| Test | Seen |
|---|---|
| `tddy-session-lifecycle::managed_codebase_workflow_acceptance::resuming_a_managed_claude_cli_session_re_wires_orchestration_and_socket` | #651 attempt 1 |
| `tddy-telegram-control::telegram_start_cursor_acceptance::start_cursor_model_callback_launches_cursor_cli_with_hooks` | #651 attempt 1 |
| `tddy-session-lifecycle::claude_cli_permission_mode_acceptance::start_session_rpc_threads_permission_mode_to_pty` | #651 attempt 2 |
| `tddy-session-lifecycle::task_service_acceptance::cancel_task_cancels_a_bash_pty_task` | #651 attempt 2 |
| `tddy-daemon::relay_idle_wired_acceptance::rpc_call_bumps_idle_tracker_so_shutdown_is_not_triggered` | #645 |

The attempts varied: one of four had no flaky test at all, one had four. The worktree-size test
([its own entry](2026-09-24-worktree-size-reload-test-races-the-persist-it-reads.md)) flaked in three of the four and
is not repeated here.

Nothing is known beyond the names. The one that was run repeatedly, `relay_idle_wired_acceptance`, passed 25 of 25
locally, and its CI failure message (`idle tracker must be reset after an RPC call`) points at timing. By name, several
of the others involve a PTY or a spawned child (`…threads_permission_mode_to_pty`, `cancel_task_cancels_a_bash_pty_task`,
launching the cursor CLI), the usual source of load sensitivity on a shared runner. None of the five tests was read, so
this is a lead, not a finding.

## What would close it

Per test, when it recurs: the failing try's stderr from the job log (the first try's output is in the log even when
the retry passes), then a local loop under load. [ci.md](../guides/ci.md) says to lower or remove the retries once the
load-sensitive tests are fixed, so this list is also the list of what is keeping them.

**Why deferred:** each flaked once and recovered; there is nothing to act on without a second sighting.
