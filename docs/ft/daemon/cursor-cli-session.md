# Cursor Agent CLI Session

> **Related:** [Claude Code CLI session](claude-cli-session.md) — sibling session type; shares `CliSessionManager`, `StreamSessionTerminalIO`, and web terminal mounting.

## Summary

A session type that spawns an interactive Cursor Agent CLI process (`agent` binary) in a PTY within a git worktree, managed entirely by `tddy-daemon`. Users select **Cursor Agent CLI** as the session type from the web **Create session** pane, choose a Cursor-supported model, and get direct terminal access — without TDD workflow overhead. The daemon handles worktree creation, `.cursor/hooks.json` installation, PTY lifecycle, activity status, session persistence, and resume if the process exits.

## User Story

As a developer, I want to start a raw Cursor Agent CLI session from the tddy web UI (or Telegram) so I can interact with the Cursor agent interactively inside a managed worktree — with the same session persistence, activity status, and resume capabilities as Claude Code CLI sessions.

## Start session

1. The **Create session** pane's **Session type** selector includes **Cursor Agent CLI** (alongside **Managed session** and **Claude Code CLI**). When selected:
   - The **Workflow recipe** dropdown is hidden.
   - A **Model** dropdown is populated from `ListAgentModels("cursor-cli")` (curated catalog in `tddy_core::cursor_cli_models()`).
   - The **Tool** dropdown is hidden (binary resolved from `cursor_cli.binary_path`, default `"agent"`).
2. **Start New Session** issues `StartSession` with `session_type = "cursor-cli"` and the chosen `model`.
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

No new RPCs. `sandbox = true` with `session_type = "cursor-cli"` returns `FAILED_PRECONDITION`.

## Out of scope (v1)

- **Sandbox mode** for cursor-cli sessions
- **`WaitingForInput`** activity status
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
