# 2026-10-17 — On Tddy Desktop, cursor-cli hooks still report to the sign-in callback port

**Category:** Known gap (found while fixing the same bug for claude-cli and split-agent hooks)
**Source:** `fix/desktop-claude-hooks-reach-daemon` — claude-cli and split-agent hooks on an embedded
daemon now pass `--daemon-socket`; cursor-cli hooks were left out of that change.

## The problem

An embedded daemon (Tddy Desktop) serves no HTTP RPC. There, `listen.web_port` is only the loopback
port a GitHub sign-in returns on. That port answers every other path with an empty 404.

`install_cursor_hooks_in_worktree` (`packages/tddy-agent-launch/src/chat.rs`) builds
`.cursor/hooks.json` through `build_cursor_hooks_settings` with `daemon_socket: None`. So every hook
posts its `ReportSessionStatus` / `ReportAgentActivity` to `daemon_url`, which resolves to that port.
`tddy-tools session-hook` swallows the 404 under its fail-quiet contract. As a result, a cursor-cli
session on the desktop records no `agent-activity.jsonl`, its Activity pane never appears, and its
status never moves.

claude-cli hooks had the same bug. They now report over the daemon's agent-tool socket, which
`tddy_daemon_kernel::embedded_host_agent_tool_socket(data_dir)` chooses when something is listening
on it.

## Why deferred

The same installer has three callers, and two of them cannot simply be handed the socket:

- **Sandboxed cursor sessions** (`packages/tddy-agent-launch/src/svc_start_sandboxed_cursor_cli_session.rs`).
  Their hooks can run inside the jail. The agent-tool socket lives under the host's `$TMPDIR`, which the
  jail may not be able to reach. A path that only resolves on the host would make every report
  in a sandboxed session fail, and fail quietly.
- **`tddy-telegram-control`** (`telegram_session_control/session_start.rs`). It calls the installer
  with the configuration only. It has no access to the data dir the daemon *resolved*
  (`TDDY_DATA_DIR` included). The socket path is a digest of that dir, so re-reading the
  configuration could produce a different path than the one the daemon bound.
- The non-sandboxed spawn (`packages/tddy-agent-launch/src/cursor_cli_spawn.rs`) could take the socket
  today, but fixing one caller of three would leave the installer's behavior depending on who called it.

## What would close it

1. Give `install_cursor_hooks_in_worktree` a `daemon_socket: Option<&str>` parameter, or the resolved
   `tddy_data_dir`, and let each caller decide. Do not resolve it inside the installer.
2. Non-sandboxed spawn: pass `embedded_host_agent_tool_socket(tddy_data_dir)`, as
   `install_claude_cli_hooks` does.
3. Sandboxed spawn: either bind-mount the agent-tool socket into the jail at a path the hook is
   told, or route the reports through the jail's own sandbox tool socket (its relay already speaks
   the same `tddy-rpc` framing). Until one of those exists, pass `None` explicitly.
4. Telegram: thread the daemon's resolved data dir through to its session start.
5. Add a test in the style of `on_an_embedded_host_the_tool_use_hook_reports_over_the_agent_tool_socket`
   (`packages/tddy-agent-launch/src/claude_cli_spawn_steps/claude_cli_spawn_steps_tests.rs`) for the
   cursor `hooks.json`. Then remove the `TODO` in `chat.rs` and delete this file.
