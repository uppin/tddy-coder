# 2026-11-03 — Cursor CLI sessions launch from cursor.yaml, on the host and in the jail

**Type:** Feature

`#agent-tools` 7/12 ([#796](https://github.com/uppin/tddy-coder/pull/796)). Packages:
`tddy-coding-agent-specs`, `tddy-agent-launch`, `tddy-sandbox-runner`, `tddy-tools`, `tddy-service`,
`tddy-session-lifecycle`.

- `specs/cursor.yaml`: binary `agent` (`TDDY_CURSOR_AGENT`), argv `[--resume {chat_id}] [--model]
  <slots> [{prompt}]` with the `node index.js` launcher layout, placed `cursor-mcp-json`, the
  managed-workflow rules file, the `cursor-hooks-json` hooks adapter, `unsupported` tool slots, and
  the jail sections (persistent `cursor` home, seed-once credentials, overlays, `network.egress.hosts`).
- Slots: `McpSlot::Placed`, `McpFormat::CursorMcpJson`, `InstructionsSlot.rules_file`,
  `SlotInputs.home`, `SlotError::{NoDaemonSocket, UnboundHome}`, `HooksAdapter::try_render`.
- Launch: host start and resume through `launch_host_pty`; jailed start through `launch_jailed`.
  Without coding agents, every Cursor launch is `FAILED_PRECONDITION`.
- Hooks: host → the daemon's agent-tool socket; jail → `--daemon-socket` names the in-jail tool-IPC
  socket, the runner relays `ReportSessionStatus` / `ReportAgentActivity`
  (`IN_JAIL_RELAYABLE_HOOK_REPORTS`, `relays_to_the_host`), and the daemon's bridge serves them for
  the jail's own session only.
- **Not shipped (developer decision, 2026-11-03):** server daemons binding the agent-tool socket. It
  needs one socket per OS user, which needs a supervisor protocol change, so a server daemon's host
  Cursor hook reports are dropped:
  [todo](../todo/2026-11-03-server-daemon-agent-tool-socket-per-os-user.md). The held
  implementation is the local ref `wip/agent-tools-cursor-cli-server-socket`.
- **Left for approval:** the hand-built Cursor paths
  ([todo](../todo/2026-11-03-cursor-hand-built-launch-paths.md)). Code issues: oversized
  `cursor_cli_spawn.rs` (586), `svc_start_sandboxed_cursor_cli_session.rs` (549),
  `tddy-tools/src/session_hook.rs` (535), `runner.rs` (2,854); all deferred to the `#agent-tools`
  sweep node (developer, 2026-11-02).

Product docs: [Cursor CLI session](../../ft/daemon/cursor-cli-session.md).
