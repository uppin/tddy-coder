# 2026-11-07 — An ssh session's pointer is owned by the daemon: no ssh agent launch records the remote agent

**Category:** Gap (deferred from `#agent-tools 21/28`, [#829](https://github.com/uppin/tddy-coder/pull/829))
**Owner:** the change that adds an ssh agent launch
**Source:** `packages/tddy-agent-launch/src/cli_session_pointer.rs` (`record_remote_owner`, `REMOTE_AGENT_PID_FILE`)

The session pointer records its owner `{pid, host}`. For a checkout on an ssh target the intended
owner is the agent on that target: its pid comes from `.tddy/agent.pid`, which the ssh launch writes
as `$$` before it `exec`s the agent, and which `record_remote_owner` reads and removes in one bounded
ssh command, rewriting the pointer with the owner it found.

No ssh agent launch exists today: `launch_host_pty` runs the agent in a local PTY, and only the slot
files and the pointer are written on the target. So `record_remote_owner` and
`REMOTE_AGENT_PID_FILE` are implemented and tested (`cli_session_pointer_unit_tests.rs`) but have no
caller, and carry `#[allow(dead_code)]`.

Until then a remote pointer is owned by the daemon's `{pid, host}`. That is safe: `tddy-tools` on
another host skips the owner check (it checks liveness and the socket path only when the owner's host
is its own), so the pointer resolves as it did before owners existed.

What would close it: when an ssh agent launch is added, its launch script writes `$$` to
`.tddy/agent.pid` before `exec`, and the launch calls `record_remote_owner` once the agent has
started; then drop the two `allow(dead_code)`.
