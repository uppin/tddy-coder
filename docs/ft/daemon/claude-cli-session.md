# Claude Code CLI Session

## Summary

A new session type that spawns an interactive `claude` CLI process in a PTY within a git worktree, managed entirely by `tddy-daemon`. Users select **Claude Code CLI** as the session type from the web UI, choose a Claude model, and get direct terminal access — without any TDD workflow overhead. The daemon handles worktree creation, PTY lifecycle, session persistence, and resume if the process exits.

This is architecturally distinct from the existing `CodingBackend`-based sessions (where `tddy-coder` orchestrates workflow tasks and invokes `claude` per-task). A Claude Code CLI session is a **raw persistent shell** — the user drives `claude` directly.

## User Story

As a developer, I want to start a raw Claude Code CLI session from the tddy web UI so I can interact with `claude` interactively inside a managed worktree — with the same session persistence and resume capabilities as regular tddy-coder sessions, but without any TDD workflow scaffolding.

## Acceptance Criteria

### Start session
1. The start-session form shows a **Session type** toggle/selector alongside the existing **Tool** dropdown. When **Claude Code CLI** is selected:
   - The **Workflow recipe** dropdown is hidden.
   - A **Model** dropdown replaces the **Backend** dropdown, populated from `ListAgents` (filtered to `claude-cli`-capable agents).
   - The **Tool** dropdown is hidden (tool binary is always `claude`).
2. Clicking **Start New Session** issues `StartSession` with `session_type = "claude-cli"` and the chosen `model`.
3. The daemon creates a git worktree at `<repo>/.worktrees/claude-cli-<short-session-id>/` on branch `claude-cli/<short-session-id>`.
4. The daemon spawns `claude --model <model>` inside the worktree in a PTY.
5. `StartSessionResponse` returns `session_id`; no LiveKit credentials (empty strings).

### Terminal access
6. The web UI opens a `GhosttyTerminalGrpc` component connected to `StreamSessionTerminalIO` (gRPC bidirectional stream on `ConnectionService`).
7. All keyboard input and output are relayed in real time. Terminal resize events update the PTY.
8. No LiveKit connection is made for Claude Code CLI sessions.

### Session persistence
9. The session directory `~/.tddy/sessions/<session-id>/` is created with `.session.yaml` containing:
   - `session_type: claude-cli`
   - `model: <model-id>`
   - `sandbox: true` (optional; darwin Seatbelt spawn when set)
   - `repo_path: <worktree-absolute-path>`
   - `pid: <claude-process-pid>`
   - `status: active`
10. The session appears in `ListSessions` with `workflow_goal`, `workflow_state`, and `elapsed_display` showing `—` (em dash), and `model` showing the selected model.
11. The `agent` column shows `claude-cli`.

### Resume after process exit
12. When the `claude` process exits (for any reason), the daemon marks the session `status: inactive` in `.session.yaml` (PID cleared).
13. The session row shows **Resume** in the session table (same as any inactive session).
14. Clicking **Resume** issues `ResumeSession`. The daemon:
    - Reads `session_type`, `model`, and `repo_path` from `.session.yaml`.
    - Verifies the worktree still exists (errors with a clear message if not).
    - Spawns a new `claude --model <model>` PTY inside the existing worktree.
    - Updates `.session.yaml` with the new PID and `status: active`.
15. Terminal access resumes immediately via the same `StreamSessionTerminalIO` stream pattern.

### Connect (reattach)
16. When the `claude` process is still running and the web client disconnects/reconnects, `ConnectSession` finds the existing PTY and reopens the stream — no new process is spawned.

### Signal and delete
17. `SignalSession` sends the signal to the `claude` PTY process.
18. `DeleteSession` sends SIGTERM, waits for exit, removes the session directory and worktree.

## Darwin sandbox mode (`StartSessionRequest.sandbox = true`)

On macOS, `StartSession` with `session_type = "claude-cli"` and **`sandbox = true`** runs
`claude` inside a **Seatbelt jail** on the same host. The daemon still creates a git worktree
and executes tools against it via `tool_engine::execute_tool`; the sandboxed agent reaches the
codebase only through `mcp__tddy-tools__*` tool calls relayed over a host-initiated gRPC
**`SessionChannel`**. No LiveKit credentials are returned.

| Aspect | Non-sandbox claude-cli | Sandboxed claude-cli |
|--------|------------------------|----------------------|
| `claude` spawn | Direct PTY in worktree | `sandbox-exec` → `tddy-tools sandbox-runner` → PTY |
| Filesystem | Full host worktree | Read-only context dir in jail; writes via MCP tools on host |
| Network from agent | Host network | `(deny network*)`; LLM HTTP relayed as `EgressRequest` frames |
| Resume / delete | PTY respawn / SIGTERM | Stop sandbox child + relaunch runner; same worktree teardown |
| Non-macOS | Supported | `failed_precondition` (no fallback) |

`.session.yaml` records `sandbox: true`. See [connection-service.md](../../../packages/tddy-daemon/docs/connection-service.md#sandboxed-claude-code-cli-sessions) and [remote-codebase-mode.md](remote-codebase-mode.md).

### Sandboxed start (macOS)

19. `StartSession` with `session_type = "claude-cli"` and **`sandbox = true`** on macOS spawns
    `tddy-tools sandbox-runner` inside Seatbelt; the daemon dials the in-jail **`SessionChannel`**
    for terminal I/O, MCP tool exec, and LLM egress relay.
20. Sandboxed sessions return empty LiveKit credentials; terminal access uses the same
    `StreamTerminalOutput` / `SendTerminalInput` RPCs as non-sandbox claude-cli.
21. The agent reads a read-only context dir in the jail; codebase mutations flow through
    `mcp__tddy-tools__*` tool calls executed on the host worktree.
22. On non-macOS, `sandbox = true` returns `failed_precondition` (no fallback).

## Non-goals (out of scope)

- Web UI support for sandboxed sessions (service-level only; same as remote mode).
- TDD/bugfix workflow integration — Claude Code CLI sessions have no recipe.
- Telegram elicitation via the structured `PresenterObserver` / `ModeChanged` pipeline (that path is for tddy-coder workflow sessions only). Claude Code CLI elicitation alerts are delivered via the `ReportSessionStatus` / `WaitingForInput` path — see [telegram-notifications.md § Claude Code CLI session activity alerts](telegram-notifications.md#claude-code-cli-session-activity-alerts).
- Model switching after session start.
- Session chaining (`previous_session_id`) for Claude Code CLI sessions.
- LiveKit presence for Claude Code CLI session processes (no `--livekit-*` args passed).

## Architecture

### Session type discriminant

`StartSessionRequest` gains a `session_type` string field (`"tool"` default, `"claude-cli"` new). The daemon branches on this field in `connection_service.rs`. Existing sessions are unaffected (absent or empty `session_type` = `"tool"`).

### `SessionMetadata` additions

```yaml
session_type: claude-cli    # optional; absent/empty = "tool"
model: claude-opus-4-8      # optional; stored for resume
sandbox: true               # optional; darwin Seatbelt jail (macOS only)
```

`repo_path` reuses the existing field to store the **worktree absolute path** (not the main repo path, unlike tool sessions which may store the project repo root).

### Terminal streaming: `StreamSessionTerminalIO`

A new RPC on `ConnectionService`:

```proto
rpc StreamSessionTerminalIO(stream SessionTerminalInput) returns (stream SessionTerminalOutput);

message SessionTerminalInput {
  string session_token = 1;  // sent on first message; ignored on subsequent
  string session_id    = 2;  // sent on first message; ignored on subsequent
  bytes  data          = 3;
}

message SessionTerminalOutput {
  bytes data = 1;
}
```

The daemon maintains a per-session `Arc<Mutex<PtyHandle>>` in an in-memory registry keyed by `session_id`. `StreamSessionTerminalIO` looks up the PTY for the session, forks a write task (client → PTY) and read task (PTY → client), and joins both until either end closes. Multiple simultaneous web clients can attach to the same PTY (shared read broadcast).

No separate `TerminalService` endpoint is used for Claude CLI sessions; the existing `TerminalService`/`TerminalServiceVirtualTui` path is untouched.

### PTY management in daemon

New `ClaudeCliSessionManager` in `tddy-daemon`:

- **Start**: creates worktree, opens PTY (`openpty`/`posix_openpt`), `fork`/`exec` `claude --model <model>` inside the worktree, stores `PtyHandle` (master fd + PID) in the registry.
- **Monitor**: a background task watches the PID via `waitpid`; on exit, marks the session inactive in `.session.yaml` and removes the PTY from the registry.
- **Resume**: opens a fresh PTY, re-exec's `claude --model <model>` in the existing worktree, updates the registry.
- **Connect**: looks up existing `PtyHandle` — no re-exec if the process is alive.

### Worktree naming

Branch: `claude-cli/<first-8-chars-of-session-uuid>`
Worktree path: `<main-repo-root>/.worktrees/claude-cli-<first-8-chars>/`

Uses existing `create_worktree_with_retry()` from `tddy-core`.

### Web: session type selector

`ConnectionScreen` form gains a **Session type** radio/tab with two options:

| Option | Shows | Hides |
|--------|-------|-------|
| **Managed session** (default) | Tool, Backend, Recipe | — |
| **Claude Code CLI** | Model dropdown | Tool, Backend, Recipe |

The Model dropdown is a static list mapped from `ListAgents` `claude-cli` entries, or a hardcoded list if `ListAgents` does not yet return them:
- `claude-opus-4-8` → "Claude Opus 4"
- `claude-sonnet-4-6` → "Claude Sonnet 4.5"
- `claude-haiku-4-5-20251001` → "Claude Haiku 4.5"

### Web: terminal component

New `GhosttyTerminalGrpc` component wraps `GhosttyTerminal` and connects `onData`/`onResize` to a gRPC `StreamSessionTerminalIO` bidi stream via the existing Connect-Web gRPC client. The component uses the same connection-chrome pattern as `GhosttyTerminalLiveKit` (status dot, disconnect, terminate).

`ConnectionScreen` mounts `GhosttyTerminalGrpc` (instead of `GhosttyTerminalLiveKit`) when the attached session's `agent` is `claude-cli`.

## Session table display

| Column | Claude CLI session value |
|--------|--------------------------|
| Workflow | `—` |
| Goal | `—` |
| Agent | `claude-cli` |
| Model | selected model label |
| Elapsed | `—` |
| Status | `active` / `inactive` |
| Activity | `Started` / `Running` / `ExecutingTool` / `WaitingForInput` / `Done` / `Ended` (see below) |

## Session activity status via per-worktree hooks

When the daemon starts a claude-cli session it writes `.claude/settings.local.json` into the git
worktree configuring six Claude Code lifecycle hooks. Each hook invokes
`tddy-tools session-hook` which maps the event to a granular `SessionActivityStatus` and calls the
new `ReportSessionStatus` gRPC RPC. The daemon validates the per-session `hook_token`, writes
`activity_status` to `.session.yaml`, and surfaces it via `ListSessions.SessionEntry.activity_status`.

### Status mapping

| Claude hook event | `notification_type` | `activity_status` |
|---|---|---|
| `SessionStart` | — | `Started` |
| `UserPromptSubmit` | — | `Running` |
| `PostToolUse` | — | `ExecutingTool` |
| `Notification` | `permission_prompt` / `elicitation_dialog` / `idle_prompt` | `WaitingForInput` |
| `Stop` | — | `Done` |
| `SessionEnd` | — | `Ended` |
| anything else | — | no-op (hook exits 0, no RPC) |

### Hook wiring

`start_claude_cli_session` generates a per-session UUID `hook_token` and calls
`tddy_core::build_claude_hooks_settings(&HookCommandParams { tddy_tools_path, daemon_url, session_id, os_user, hook_token })`.
The resulting JSON is written to `<worktree>/.claude/settings.local.json`. The `hook_token` is also
stored in `.session.yaml` for validation on inbound `ReportSessionStatus` calls.

Config (`claude_cli:` block) may override `tddy_tools_path` (default: `current_exe` sibling → `"tddy-tools"`) and `daemon_url` (default: `http://127.0.0.1:{web_port}`).

### Fail-quiet contract

`tddy-tools session-hook` always exits 0. A 2-second reqwest timeout prevents a dead daemon from blocking Claude. On no-op events (unrecognized event names, unrecognized `notification_type`) the tool exits immediately without making any network call.

### Auth model

The hook has no web session token. A per-session random `hook_token` (UUID) is generated at session start, persisted in `.session.yaml`, and baked into each hook command line. `ReportSessionStatus` resolves `sessions_base` directly from `os_user` (bypasses the GitHub OAuth path) and constant-time-compares the token. Sessions with no `hook_token` (e.g. Telegram-started) silently ignore inbound hook reports.

### Web UI

Web-side rendering of `activity_status` (badge in the session list) is a follow-up; this changeset surfaces the field in the `ListSessions` response but does not yet display it in the UI.

## Proto delta (connection.proto)

```proto
message StartSessionRequest {
  // ... existing fields ...
  string session_type = 7;  // "tool" (default) or "claude-cli"
  string model        = 8;  // model id for claude-cli sessions
  bool   sandbox      = 15; // when true with session_type "claude-cli": darwin Seatbelt spawn (macOS only)
}

rpc StreamSessionTerminalIO(stream SessionTerminalInput) returns (stream SessionTerminalOutput);

message SessionTerminalInput {
  string session_token = 1;
  string session_id    = 2;
  bytes  data          = 3;
}

message SessionTerminalOutput {
  bytes data = 1;
}

// --- Activity status hooks ---
rpc ReportSessionStatus(ReportSessionStatusRequest) returns (ReportSessionStatusResponse);

message ReportSessionStatusRequest {
  string session_id  = 1;
  string hook_token  = 2;
  string os_user     = 3;
  string status      = 4;  // one of the SessionActivityStatus wire strings
}

message ReportSessionStatusResponse {
  bool ok = 1;
}

// SessionEntry (in ListSessionsResponse) gains:
//   string activity_status = 15;
```

`StartSessionResponse`, `ConnectSessionResponse`, and `ResumeSessionResponse` are unchanged — LiveKit fields are returned as empty strings for Claude CLI sessions. The web client detects `agent == "claude-cli"` from `ListSessions` to decide which terminal component to mount.

## Seeding a first prompt

Both the RPC path and the Telegram path support an optional **initial prompt** passed directly to the `claude` binary as a positional argument:

```
claude --model <model> --session-id <id> "build feature X"
```

### RPC: `StartSessionRequest.initial_prompt`

`StartSessionRequest` has a `string initial_prompt = 13;` field. When non-empty, `start_claude_cli_session` passes it as a positional CLI argument to `claude`. An empty string is treated as absent (no extra arg). This lets programmatic callers seed the first user turn without requiring interactive input.

### Telegram: `/start-claude <prompt>`

The Telegram `/start-claude <prompt>` flow (see [telegram-session-control.md](telegram-session-control.md)) seeds the session with the user's message. After project → branch → model selection, `spawn_telegram_claude_cli` reads `initial_prompt` from `changeset.yaml` and passes it to `ClaudeCliSessionManager::start(…, initial_prompt)`.

### Resume does not replay

`ResumeSession` always calls `ClaudeCliSessionManager::resume`, which **does not** pass the original `initial_prompt`. This prevents the first user message from being re-injected as a duplicate turn when `claude` is restarted inside an existing session.

### Implementation note

Argv construction is isolated in `build_claude_argv(binary, model, session_id, initial_prompt: Option<&str>) -> Vec<String>` in `claude_cli_session.rs`. The positional arg is appended after `--session-id` only when the trimmed prompt is non-empty.

## See also

- [Telegram session control](telegram-session-control.md) — `/start-claude` Telegram flow
- [Web terminal](../web/web-terminal.md) — GhosttyTerminal, connection chrome, session table
- [Connection service](../../../packages/tddy-daemon/docs/connection-service.md) — daemon RPC implementation
- [Worktrees](../web/worktrees.md) — worktree lifecycle, `create_worktree_with_retry`
