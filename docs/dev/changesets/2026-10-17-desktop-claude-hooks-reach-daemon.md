# 2026-10-17 — Tddy Desktop: claude-cli hooks reach the daemon

**Type:** Fix

On an embedded daemon (Tddy Desktop) `listen.web_port` is only the GitHub sign-in callback, so
claude-cli hook reports (`ReportSessionStatus`, `ReportAgentActivity`) POSTed there got an empty 404
the fail-quiet hook swallowed: no `agent-activity.jsonl`, no Activity control, no status. Hooks now
report over the embedded daemon's agent-tool socket (the one `#521` added for co-located agents).

## What changed

- **`tddy-daemon-kernel`** — `embedded_host_agent_tool_socket(data_dir)`: the socket path when
  something is *listening* there (a live connect; the embedded daemon does not unlink it on exit).
  `tddy-session-split`'s `agent_tool_socket_for_embedded_host` delegates to it.
- **`tddy-agent-backend`** — `HookCommandParams.daemon_socket`; hook commands append
  `--daemon-socket '<path>'` when set and keep `--daemon`.
- **`tddy-tools session-hook --daemon-socket`** — both reports over the socket
  (`connect_sandbox_ipc` + unary call, 2 s ceiling), still always exit 0.
- **`tddy-agent-launch`** — `install_claude_cli_hooks` takes the daemon's resolved data dir
  (`TDDY_DATA_DIR` wins over `config.tddy_data_dir`) and passes the live socket.
- **`tddy-session-split`** — split-agent hooks pass the same socket.
- Auth unchanged: both handlers check only the request's `hook_token`/`os_user`.
- **Product behaviour:** [claude-cli session](../../ft/daemon/claude-cli-session.md#which-transport-a-hook-reports-over),
  [agent activity](../../../packages/tddy-session-activity/docs/agent-activity.md).

## Deferred

- Cursor-cli hooks on the desktop:
  [`docs/dev/todo/2026-10-17-desktop-cursor-hooks-reach-daemon.md`](../todo/2026-10-17-desktop-cursor-hooks-reach-daemon.md).

## Code issues

- `tddy-session-split/docs/code-issues/complexity-svc-spawn-split-agent-spawn-split-agent.md` — not
  affected: the two added lines are outside `spawn_split_agent` (118 lines at `:57` on `master` and
  on this branch).
