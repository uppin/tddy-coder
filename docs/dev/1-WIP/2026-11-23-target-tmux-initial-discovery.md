# Initial discovery: tmux on Coder/SSH targets

## Combined conclusions

- `refuse_a_conflicting_tmux_run_start` (`packages/tddy-agent-launch/src/start_request_checks.rs`)
  refuses sandbox / sandboxed_codebase / split with tmux; no target case — so a target start passes
  and `run_in_tmux` is ignored.
- `harness_launch::launch` sends any `remote` launch to `launch_on_target` before looking at
  `HarnessRun`; the tmux branch (`cli_harness_tmux::start_harness_in_tmux`) is host-only.
- `remote_agent_command` = `cd W && mkdir -p .tddy/agents && printf %s "$$" > pid && <target_login_exec>`.
  `launch_on_target` clears the pid file, spawns the client, `judge_the_start` waits for the pid
  file (`startup::wait_for_agent_start`), probes it during the startup window, then
  `record_remote_owner` reads + removes the pid file and writes the pointer owner `{pid, host}`.
- `probe_agent_on_target` (`remote_agent_launch/agent_probe.rs`) is `command -v <cmd>` via the target
  login shell; `require_the_agent_on_the_target` maps errors with `tab_launch_status`
  (FAILED_PRECONDITION / UNAVAILABLE). Reusable for tmux.
- Resume (`opencode_cli_spawn/resume.rs`) passes `HarnessRun::resume(meta.tmux_session.is_some())`
  and a `HarnessRemote` for target sessions — same split as start.
- Delete (`svc_signal_delete_session.rs::kill_recorded_tmux_session`) kills **local** tmux whenever
  metadata records `tmux_session` — would be wrong for a target session.
- `tddy-session-driver` `Session::stop` kills the remote agent by pid file.
- Pointer owner pid is used by `held_pointer` (agent-backend) for liveness on the owner's host —
  a tmux pane pid (pane command `exec`s the agent) serves.
- tmux tabs: `svc_terminal_ports/tmux_host.rs::HostTmux` lists daemon-host tmux only; no target path.
- tmux session name: `tddy_cli_sessions::tmux::agent_tmux_session_name` (`tddy-<id>`, separators
  stripped).

## Exploration 1

Read: start_request_checks.rs:110-200, opencode_cli_spawn/harness_launch.rs:110-200,
remote_agent_launch.rs:1-140, 255-545, remote_agent_launch/agent_probe.rs, target_start_agent.rs:40-110,
opencode_cli_spawn.rs:330-420, opencode_cli_spawn/resume.rs (grep target|tmux),
svc_signal_delete_session.rs:280-360, tmux_agent_end.rs, cli_session_pointer.rs:195-260,
tddy-session-driver/src/session.rs:290-340, tddy-agent-backend session_pointer.rs / turn_files.rs,
svc_terminal_ports/tmux_host.rs. Greps: `run_in_tmux|in_tmux`, `remote_agent_pid_file`,
`kill_tmux_session`, `.owner`, `TmuxHost`.
