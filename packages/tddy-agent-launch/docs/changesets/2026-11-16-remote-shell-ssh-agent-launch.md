# 2026-11-16 — A session's agent runs on its SSH host

**Type:** Feature · `#remote-shell` 3/7, PR [#917](https://github.com/uppin/tddy-coder/pull/917)

`remote_agent_launch` launches a session's agent on its target through the remote-shell port:
`shell_for_target` resolves an SSH target to its transport, `remote_agent_command` enters the
worktree, records the shell's pid in `.tddy/agent.pid` and `exec`s the agent under exactly its
spec's environment, and `launch_on_target` serves the transport's interactive command from a
local PTY, fails a start whose agent exits inside the startup window with its last output, and
records the remote agent as the session pointer's owner via `record_remote_owner` (whose
`allow(dead_code)`, and the one on `REMOTE_AGENT_PID_FILE`, are removed). The harness launch
takes the remote branch from `start_cli_harness_session`'s `HarnessRemote`. Full record:
[`docs/dev/changesets/2026-11-16-remote-shell-ssh-agent-launch.md`](../../../../docs/dev/changesets/2026-11-16-remote-shell-ssh-agent-launch.md).
