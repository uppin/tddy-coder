# 2026-10-24 — Exit hooks on a CLI-harness launch

**Type:** Feature

`#warp-agent` 1/2, PR [#717](https://github.com/uppin/tddy-coder/pull/717). Cross-package entry:
[2026-10-24-warp-cli-session.md](../../../../docs/dev/changesets/2026-10-24-warp-cli-session.md). See [module layout § Exit hooks](../module-layout.md#exit-hooks).

- `launch_cli_harness(harness, session_id, worktree, &CliLaunch, HarnessLaunchExtras { args, env, on_exit })`
  replaces the positional `extra_args` and `env` arguments.
- `ExitHook` (`pty_spawn.rs`, re-exported): a boxed `FnOnce` given the terminal capture and returning a
  future. `spawn_tool` takes it; the terminal-cleanup task runs it once the process has exited and its
  output is complete, before the terminal leaves the index.
- `CliSessionManager::exit_hook_settled(session_id)` waits for the main terminal's pending hook when its
  process has exited; returns at once with no hook pending or while the process runs (its PTY control is
  still registered).

Tests: `tests/exit_hook.rs` — `a_resume_waits_for_the_exit_hook_of_the_run_before`,
`a_resume_does_not_wait_for_a_run_that_is_still_going`.
