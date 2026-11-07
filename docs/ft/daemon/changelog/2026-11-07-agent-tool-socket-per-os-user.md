# 2026-11-07 — Server daemons give each session user an agent-tool socket of their own

`#agent-tools` 26/28 ([#834](https://github.com/uppin/tddy-coder/pull/834)).

- **Host hooks report on server daemons.** A `tddy-daemon` binary serves one agent-tool socket per
  session OS user, `<data dir>/run-agent-tools/<user>/agent.sock` (directory `0700`, socket `0600`,
  owned by that user). Claude's and Cursor's hooks, `tddy-tools notify` and a managed agent's tool
  server reach the daemon over it, so a host Cursor session's activity status updates on a server
  daemon, and no other user can connect to another's socket. Tddy Desktop keeps its one socket. See
  [claude-cli-session.md § Which transport a hook reports over](../claude-cli-session.md#which-transport-a-hook-reports-over)
  and [cursor-cli-session.md § Activity status hooks](../cursor-cli-session.md#activity-status-hooks-cursorhooksjson).
- **Under `tddy-supervisor` the supervisor makes the sockets**, from `agent_tool_sockets:` in
  `supervisor.yaml`. A session for a user with no entry is **refused at start**
  (`FAILED_PRECONDITION`, naming `agent_tool_sockets`), not started with hooks that report nowhere.
  See [tddy-supervisor.md § Per-user sockets](../../supervisor/tddy-supervisor.md#per-user-sockets-handed-to-the-daemon).

**Upgrade.** On a host run by `tddy-supervisor`, every session user needs an `agent_tool_sockets:`
entry, or their session starts are refused. Re-running `sudo ./install --systemd` writes them: while
the `tddy-daemon` service declares no `agent_tool_sockets:` block, install adds one with
`{user: <u>, path: /run/tddy-agent-tools/<u>/agent.sock}` for each `spawn_policy.allowed_session_users`
user. An existing block is left as written, and install warns naming each policy user it is missing.
Each user's two sockets (host-session and agent-tool) count together against the supervisor's 15
handed-over sockets, so seven users with both fit one daemon. See
[systemd-install.md § Behavior notes](../systemd-install.md#behavior-notes).
