# Cursor Agent CLI Session

> **Related:** [Claude Code CLI session](claude-cli-session.md) — sibling session type; shares `CliSessionManager`, `StreamSessionTerminalIO`, and web terminal mounting.

## Summary

A session type that spawns an interactive Cursor Agent CLI process (`agent` binary) in a PTY within a git worktree, managed entirely by `tddy-daemon`. Users select **Cursor Agent CLI** as the session type from the web **Create session** pane, choose a Cursor-supported model, and get direct terminal access — without TDD workflow overhead. The daemon handles worktree creation, `.cursor/hooks.json` installation, PTY lifecycle, activity status, session persistence, and resume if the process exits.

## User Story

As a developer, I want to start a raw Cursor Agent CLI session from the tddy web UI (or Telegram) so I can interact with the Cursor agent interactively inside a managed worktree — with the same session persistence, activity status, and resume capabilities as Claude Code CLI sessions.

## Start session

1. The **Create session** pane's **Session type** selector includes **Cursor Agent CLI** (alongside **Managed session** and **Claude Code CLI**). When selected:
   - The **Workflow recipe** dropdown appears when **Managed codebase** is enabled.
   - A **Model** dropdown is populated from `ListAgentModels("cursor-cli")` (curated catalog in `tddy_core::cursor_cli_models()`).
   - The **Tool** dropdown is hidden (binary resolved from `cursor_cli.binary_path`, default `"agent"`).
   - **Sandbox** and **Managed codebase** toggles are available (parity with Claude CLI).
2. **Start New Session** issues `StartSession` with `session_type = "cursor-cli"`, the chosen `model`, and optional `sandbox`, `managed_codebase`, `recipe`, and `specialized_agents`.
3. The daemon creates a git worktree on branch `cursor-cli/{short_id}` (auto-default when `new_branch_name` is empty).
4. The daemon writes `<worktree>/.cursor/hooks.json` via `tddy_core::build_cursor_hooks_settings`.
5. The daemon spawns the Cursor Agent CLI with `--model <model>` and an optional initial prompt.
6. `StartSessionResponse` returns `session_id`; LiveKit fields are empty.

## Terminal access

- The web UI mounts `GhosttyTerminalGrpc` connected to `StreamSessionTerminalIO` (same component path as claude-cli).
- No LiveKit connection is made for Cursor CLI sessions.

## Session persistence

The session directory `~/.tddy/sessions/<session-id>/` contains `.session.yaml` with:

- `session_type: cursor-cli`
- `model: <model-id>`
- `repo_path: <worktree-absolute-path>`
- `pid: <agent-pid>`
- `hook_token: <per-session-uuid>`
- `status: active`

`ListSessions` shows `workflow_goal`, `workflow_state`, and `elapsed_display` as `—`, `model` as the selected model, and `agent` as `cursor-cli`.

## Resume, connect, signal, delete

- **Resume** — when the agent process exits, status becomes `inactive`; **Resume** re-spawns in the existing worktree without replaying the initial prompt.
- **Connect** — when the process is still running, `ConnectSession` reattaches to the existing PTY.
- **Signal** — `SignalSession` delivers to the agent PTY process.
- **Delete** — SIGTERM, wait, remove session directory and worktree.

## Activity status hooks (`.cursor/hooks.json`)

At session start the daemon writes `<worktree>/.cursor/hooks.json`. Each hook invokes `tddy-tools session-hook`, which reads Cursor's stdin JSON `hook_event_name`, maps to `SessionActivityStatus`, and calls `ReportSessionStatus`. The daemon validates `hook_token`, writes `activity_status` to `.session.yaml`, and surfaces it via `ListSessions`.

| Cursor hook event | `activity_status` |
|---|---|
| `sessionStart` | `Started` |
| `beforeSubmitPrompt` | `Running` |
| `postToolUse` | `ExecutingTool` |
| `stop` | `Done` |
| `sessionEnd` | `Ended` |
| `preToolUse` / `beforeShellExecution` / `afterShellExecution` / others | no-op (exit 0, no RPC) |

**`WaitingForInput`** is not produced — Cursor hooks have no permission-prompt equivalent in v1.

Hooks use a fail-quiet contract (2-second reqwest timeout, exit 0 on no-op events), matching the Claude CLI hook path.

## Telegram `/start-cursor`

`/start-cursor <prompt>` follows **project → branch → model** (`tcur:` callbacks), then spawns with the prompt as a positional arg. See [telegram-session-control.md](telegram-session-control.md#start-cursor-flow).

## Architecture

### Session type discriminant

`StartSessionRequest.session_type` accepts `"cursor-cli"`. The daemon branches in `connection_service.rs` via `cursor_cli_spawn.rs`. Existing `tool` and `claude-cli` sessions are unaffected.

### Session manager

`CliSessionManager` in `packages/tddy-daemon/src/cli_session_manager.rs` (re-exported as `claude_cli_session`) handles both `claude-cli` and `cursor-cli` PTY sessions. `start_cursor()` sets `PtyHandle.kind = "cursor-cli"`.

### Hook wiring

- `tddy_core::build_cursor_hooks_settings` — emits `.cursor/hooks.json` v1 (five lifecycle events).
- `tddy-tools session-hook` — primary mapping from stdin `hook_event_name`; `--event` fallback for tests.
- `tddy_core::session_activity` — unified PascalCase + camelCase hook name → status mapping shared with Claude hooks.

### Config

Optional `cursor_cli:` block in `daemon.yaml`:

- `binary_path` — Cursor Agent CLI binary (default `"agent"`)
- `tddy_tools_path` — path to `tddy-tools` for hook commands
- `daemon_url` — base URL for `ReportSessionStatus` (defaults to `http://127.0.0.1:{web_port}`)

### Web UI

**CreateSessionPane** is the primary entry point for the third session type. Terminal mount helpers in `ConnectionScreen` and `SessionsDrawerScreen` treat `cursor-cli` like `claude-cli` for gRPC terminal attachment.

### Proto

No new RPCs. `sandbox = true` with `session_type = "cursor-cli"` starts a Seatbelt (macOS) or cgroups+namespaces (Linux) sandboxed session via `start_sandboxed_cursor_cli_session`, mirroring `claude-cli`.

### Sandbox mode

When `StartSession.sandbox = true`:

- macOS: Seatbelt profile from `tddy-sandbox-recipes::cursor_cli` via `tddy-sandbox-runner --agent-kind cursor`.
- Linux: rootless cgroups+user namespaces (same backend as Claude CLI).
- MCP config: writes `$HOME/.cursor/mcp.json` inside the jail (registering `tddy-tools --mcp`). Headless approval flags (`--approve-mcps`, `--force`, `--trust`) are **not** injected by tddy — pass them explicitly via agent args when using print mode.
- Egress is confined to the SessionChannel tunnel; tool execution uses the host worktree via IPC.

### Managed codebase and specialized subagents

When `managed_codebase = true`, the daemon seeds `changeset.yaml`, writes orchestration guidance to `.cursor/rules/tddy-managed-workflow.mdc` in the worktree (or prepends it to the initial prompt for non-sandbox sessions), sets `TDDY_SOCKET`, and exposes `subagent_new_session` / `prompt` / `cancel` via MCP. `specialized_agents` resolves to `TDDY_SUBAGENTS_JSON` / `TDDY_SUBAGENT` in the sandbox env overlay.

### Workflow `CursorBackend::invoke`

`CursorBackend::invoke` registers `tddy-tools --mcp` and `permission-prompt-tool` for headless tool approvals (same model as `ClaudeCodeBackend`), and exports `TDDY_SOCKET`, `TDDY_REPO_DIR`, `TDDY_SESSION_DIR`, and `TDDY_REMOTE_*` (`RemoteToolEnv`) into the invoke subprocess.

## Known follow-ups

- **Jail authentication:** interactive `agent` in a Seatbelt jail may fail to store tokens in macOS Keychain (exit 155). Hosts that store credentials only in Keychain need `AGENT_CLI_CREDENTIAL_STORE=file` (writes `~/.cursor/auth.json`) or `CURSOR_API_KEY` for headless runs — not yet wired into the sandbox env overlay.
- **`resume_sandboxed_cursor_cli_session`:** sandboxed cursor-cli resume relaunch is not implemented; non-sandbox resume works via `CliSessionManager`.

## Out of scope (v1)

- **`WaitingForInput`** activity status (Cursor hooks have no permission-prompt equivalent — documented gap)
- TDD/bugfix workflow integration (no recipe)
- Model switching after session start
- Session chaining (`previous_session_id`)
- LiveKit presence for cursor-cli processes
- **`ConnectionScreen`** legacy inline start form third session type (CreateSessionPane is primary)

## References

- [Claude Code CLI session](claude-cli-session.md)
- [Connection service](../../../packages/tddy-daemon/docs/connection-service.md)
- [Telegram session control](telegram-session-control.md)
- [Cursor Hooks docs](https://cursor.com/docs/hooks.md)
- [Cursor CLI headless docs](https://cursor.com/docs/cli/headless)
