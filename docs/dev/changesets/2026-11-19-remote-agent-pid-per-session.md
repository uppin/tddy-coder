# 2026-11-19 — A target agent's pid is its session's own, `--binary` reaches the target, and a Coder session ends with its agent's status

**Type:** Fix · **Packages:** `tddy-agent-launch`, `tddy-remote-shell`, `tddy-pty`,
`tddy-terminal-rpc`, `tddy-cli-sessions`, `tddy-session-driver` (and one `PtySpawnSpec` literal in
`tddy-coder`)

## Problem (live on `mantasi/remote-dev`, 2026-11-19)

A manual check of `tddy-session-driver` against a real Coder workspace (a login `bash`, then Claude)
found three gaps:

1. **One pid file per worktree.** The remote command recorded every agent's pid at
   `<worktree>/.tddy/agent.pid`. Two sessions sharing a worktree (several remote agents in one
   checkout, or the driver pointed at one directory twice) overwrote each other's pid, so a stop
   killed whichever agent wrote last. The driver's `stop` also left the file behind after killing
   by it, so a later stop could signal whatever process had reused the pid.
2. **`--binary` never reached a target.** `main.rs` set the override on `agent.tool.binary`, but
   since #954 a target launch runs the tool's bare `command` on the target's `PATH`
   (`remote_agent_launch/session_files.rs`), so the workspace ran the spec's own command:
   `env: 'warp': No such file or directory`, exit 127.
3. **A Coder session's exit status was always 1.** `coder ssh` (v2.33.5, no flag to change it)
   exits 1 for any command that failed and names the command's status only in its last line,
   `error: run command: Process exited with status 3`. The session's terminal reported the
   client's 1 — to the driver's `session_status`, to a launch that failed during its startup
   window, and to the daemon's `main exited` line and `AgentEnd`.

## Change

- **Per-session pid file.** `cli_session_pointer::remote_agent_pid_file(session_id)` is
  `.tddy/agents/<session id>.pid`, replacing the `REMOTE_AGENT_PID_FILE` constant.
  `remote_agent_command(worktree, session_id, argv, env)` writes it (`mkdir -p .tddy/agents`,
  single-quoted path); `launch_on_target`'s first call clears that file only; `wait_for_agent_start`
  and `record_remote_owner` take the session id. The driver's `stop` kills by its session's file and
  removes it in the same command. The pid-directory preparation is its own function,
  `clear_the_agent_pid_file`.
- **`--binary` on a target.** The driver sets the override on both `tool.binary` (what a local
  launch runs) and `tool.command` (what a target launch runs).
- **The agent's status through a client that hides it.** `tddy_remote_shell::InteractiveCommand`
  gains `exit: InteractiveExit` — `ClientStatus` (OpenSSH, the in-memory double) or
  `ReportedLast { client_code, marker }`. `CoderShell::interactive` declares
  `ReportedLast { 1, "Process exited with status " }` (`CoderShell::FAILED_COMMAND_EXIT_CODE`,
  `FAILED_COMMAND_STATUS`). `launch_on_target` maps it to `tddy_pty::ExitCodeInOutput` and spawns
  through the new `CliSessionManager::spawn_rendered_client_main`; `tddy_pty::PtySpawnSpec` (and
  `tddy_terminal_rpc`'s spec) carry `exit_code_in_output`. Once the child has exited and its output
  has drained, the PTY runtime reads the status after the **last** marker in the last 4 KiB of
  output — only when the child exited `client_code`; a clean exit, a signal, another code, or a
  failure with no marker (the client itself failed) keeps the child's own status. Every reader of
  the terminal's status therefore sees the agent's.

## Acceptance tests

- `tddy-session-driver` `session_driver_acceptance`:
  `two_sessions_sharing_a_worktree_each_stop_only_their_own_agent`,
  `a_binary_named_for_a_coder_target_is_the_one_the_workspace_runs`,
  `a_coder_session_ends_with_its_agent_s_exit_status_not_the_cli_s` (a stand-in `coder` that exits 1
  and names the status last, as the real CLI does), and
  `stopping_a_coder_session_stops_its_agent_on_the_workspace` now also checks the pid file is gone.
- `tddy-agent-launch`: `agents_sharing_a_worktree_each_record_their_own_pid`; the launch acceptance
  suites look for the per-session file.
- `tddy-pty` `exit_status`: the reported status is read, the last report wins, and a status the
  client did not attribute to the command is kept.
- `tddy-remote-shell`: the Coder interactive command declares `ReportedLast`; ssh declares
  `ClientStatus`.

## Live check

Against `mantasi/remote-dev` (Coder v2.33.5), through `tddy-session-driver` with a login `bash` as the
agent: the cd/`pwd`/`git status`/`git log`/`ls` sequence, multi-line input, exit codes and stdin
round-trips all came back. `exit 3` ended the session with `exit: "exit status 1"` before this change
and `"exit status 3"` after it; the pid was recorded at `~/.tddy/agents/<session id>.pid` and removed by
the stop; `--agent warp --binary bash` ran `bash` on the workspace where it had failed with
`env: 'warp': No such file or directory`. A Claude session on a detached qape-hq worktree answered a
prompt and stopped cleanly.

## Code issues (re-measured)

| Record | Before | After | Verdict |
|---|---:|---:|---|
| `tddy-agent-launch` `complexity-remote-agent-launch-launch-on-target` | 60 lines, 6 params | 43 lines, 6 params | narrowed — still open on parameters |
| `tddy-pty` `oversized-file-runtime` | 523 production lines | 533 | regressed (the spec field, its import and the call site; the read lives in `exit_status.rs`) |
| `tddy-cli-sessions` `complexity-pty-spawn-spawn-tool-on` | 75 lines, 12 params | 77 lines, 13 params | worse (the `exit_code_in_output` parameter) |

No `docs/dev/todo/` entry is resolved or touched by this change.

## Known limits

- The marker is Coder's wording; a CLI release that rewords it falls back to the client's 1, never
  to a wrong code. The pty test pins the wording.
- An agent that itself prints `Process exited with status N` as its last output and then makes the
  client exit 1 would be read as `N`. Only the last report counts, and only when the client exited 1.
- The driver's `stop` still kills the recorded pid without checking it is the agent (pid reuse
  between start and stop on the target). The window is now one session's own file.
