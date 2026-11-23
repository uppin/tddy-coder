# 2026-11-23 — The agent of a target session runs in tmux on the target (`#target-tmux` 1/2, #1019)

**Type:** Feature · **Packages:** tddy-agent-launch, tddy-session-lifecycle, tddy-session-driver, tddy-testing-commons

## What changed

- `RemoteLaunch::tmux` (no new parameter on `launch_on_target`); `remote_agent_tmux_command`,
  `target_tmux_session_name`, `end_target_tmux_session`, `probe_tmux_on_target`,
  `RemoteLaunchError::TmuxNotOnTarget` (→ `FAILED_PRECONDITION`).
- `require_tmux_on_the_target` runs on start and resume of a target tmux run, before any worktree.
- `RemoteAgentPid { File, TmuxPane }` (`cli_session_pointer.rs`): the start wait and the pointer owner
  read the pid from the pid file or from tmux (`display-message -t '=<name>:'` — `=<name>` alone
  names no pane and prints an empty line).
- Delete (`target_tmux_run_of`) kills the tmux session on the target through the session's shell;
  resume's unwatched-exit settle and the startup `pre_roster_main` skip target sessions.
- `StandInSshTarget` gives each stand-in machine a private `TMUX_TMPDIR`; its login shell accepts `-c`
  inside tmux.

## Decisions (developer)

The tmux session name is the identity, not a pid; a dropped connection goes inactive and Resume
re-attaches; Stop detaches (as for host tmux runs); tmux tabs on targets are node 2.

## Verification

`./test -p tddy-agent-launch` green (remote acceptance 18/18); `remote_target_session_acceptance`
17/17 at the time (4 new daemon tests); clippy clean on the touched crates.

## Deferred

- Automatic reconnect: [`docs/dev/todo/2026-11-23-target-tmux-auto-reconnect.md`](../todo/2026-11-23-target-tmux-auto-reconnect.md).
- `remote_agent_launch.rs` 529 → 623 production lines: split deferred until the stack lands (node 2
  edits the same file) — `packages/tddy-agent-launch/docs/code-issues/oversized-file-remote-agent-launch.md`.
