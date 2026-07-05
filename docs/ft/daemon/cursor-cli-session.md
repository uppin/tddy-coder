# Cursor Agent CLI Session

## Summary

A new session type that spawns an interactive `cursor-agent` (Cursor Agent CLI) process in a PTY
within a git worktree, managed entirely by `tddy-daemon`. Users select **Cursor Agent CLI** as the
session type from the web UI, choose a Cursor-supported model, and get direct terminal access —
without any TDD workflow overhead. The daemon handles worktree creation, PTY lifecycle, session
persistence, activity-status hooks, and resume if the process exits.

This is a sibling of the existing **Claude Code CLI** session (`claude-cli-session.md`), mirroring
its architecture but spawning `cursor-agent` instead of `claude` and using Cursor's `.cursor/hooks.json`
lifecycle-hook system instead of Claude's `.claude/settings.local.json` hooks. A parallel Telegram
`/start-cursor` flow is included.

## Background and motivation

`tddy-daemon` already supports raw interactive Claude Code CLI sessions. Cursor recently shipped a
standalone Cursor Agent CLI (`cursor-agent`, installed via `curl https://cursor.com/install -fsS | bash`)
that runs the same agent loop as the Cursor IDE in a terminal. Supporting it as a first-class session
type lets developers drive Cursor interactively inside a managed worktree from the tddy web UI or
Telegram, with the same persistence, resume, and activity-status surfaces as Claude CLI sessions.

## User Story

As a developer, I want to start a raw Cursor Agent CLI session from the tddy web UI (or Telegram) so
I can interact with `cursor-agent` interactively inside a managed worktree — with the same session
persistence, activity status, and resume capabilities as Claude Code CLI sessions, but driving the
Cursor agent instead.

## Affected features

- [`docs/ft/daemon/claude-cli-session.md`](claude-cli-session.md) — sibling session type; this feature
  mirrors its shape. The shared `StreamSessionTerminalIO` RPC, `PtyRuntime`/`TaskRegistry` plumbing,
  and session-table display are reused unchanged.
- [`docs/ft/daemon/telegram-session-control.md`](telegram-session-control.md) — gains a
  `/start-cursor` flow parallel to `/start-claude`.
- [`docs/ft/daemon/telegram-notifications.md`](telegram-notifications.md) — Cursor CLI session
  activity alerts follow the same `ReportSessionStatus` / `WaitingForInput` path as Claude CLI.

## High-level requirements

### Start session
1. The start-session form's **Session type** selector gains a third option, **Cursor Agent CLI**
   (alongside **Managed session** and **Claude Code CLI**). When selected:
   - The **Workflow recipe** dropdown is hidden.
   - A **Model** dropdown replaces the **Backend** dropdown, populated from `ListAgents` filtered to
     `cursor-cli`-capable agents (hardcoded fallback list if `ListAgents` does not yet return them).
   - The **Tool** dropdown is hidden (tool binary is always `cursor-agent`).
2. Clicking **Start New Session** issues `StartSession` with `session_type = "cursor-cli"` and the
   chosen `model`.
3. The daemon creates a git worktree at `<repo>/.worktrees/cursor-cli-<short-session-id>/` on branch
   `cursor-cli/<short-session-id>`.
4. The daemon writes `.cursor/hooks.json` into the worktree (see Activity status hooks).
5. The daemon spawns `cursor-agent` inside the worktree in a PTY.
6. `StartSessionResponse` returns `session_id`; no LiveKit credentials (empty strings).

### Terminal access
7. The web UI mounts the same `GhosttyTerminalGrpc` component used for Claude CLI sessions,
   connected to `StreamSessionTerminalIO`.
8. No LiveKit connection is made for Cursor CLI sessions.

### Session persistence
9. The session directory `~/.tddy/sessions/<session-id>/` is created with `.session.yaml` containing:
   - `session_type: cursor-cli`
   - `model: <model-id>`
   - `repo_path: <worktree-absolute-path>`
   - `pid: <cursor-agent-pid>`
   - `hook_token: <per-session-uuid>`
   - `status: active`
10. The session appears in `ListSessions` with `workflow_goal`, `workflow_state`, and
    `elapsed_display` showing `—` (em dash), `model` showing the selected model, and `agent` showing
    `cursor-cli`.

### Resume after process exit
11. When the `cursor-agent` process exits, the daemon marks the session `status: inactive` (PID
    cleared). The session row shows **Resume**. Clicking **Resume** re-spawns `cursor-agent` inside
    the existing worktree and updates `.session.yaml`. Resume does not replay any initial prompt.

### Connect (reattach)
12. When the `cursor-agent` process is still running and the web client reconnects, `ConnectSession`
    finds the existing PTY and reopens the stream — no new process is spawned.

### Signal and delete
13. `SignalSession` sends the signal to the `cursor-agent` PTY process.
14. `DeleteSession` sends SIGTERM, waits for exit, removes the session directory and worktree.

### Activity status hooks (`.cursor/hooks.json`)
15. At session start the daemon writes `<worktree>/.cursor/hooks.json` configuring Cursor lifecycle
    hooks. Each hook invokes `tddy-tools session-hook` (Cursor event variant) which maps the event
    to a granular `SessionActivityStatus` and calls `ReportSessionStatus`. The daemon validates the
    per-session `hook_token`, writes `activity_status` to `.session.yaml`, and surfaces it via
    `ListSessions.SessionEntry.activity_status`.

    Status mapping:

    | Cursor hook event | `activity_status` |
    |---|---|
    | `sessionStart` | `Started` |
    | `beforeSubmitPrompt` | `Running` |
    | `postToolUse` | `ExecutingTool` |
    | `stop` | `Done` |
    | `sessionEnd` | `Ended` |
    | `preToolUse` / `beforeShellExecution` / `afterShellExecution` / others | no-op (hook exits 0, no RPC) |

16. `WaitingForInput` is **not** produced by Cursor hooks in v1 (Cursor has no permission-prompt
    notification hook equivalent to Claude's `Notification` event). The status will simply not
    transition to `WaitingForInput` for Cursor CLI sessions; this is an accepted v1 gap documented
    in the changeset.
17. Hooks communicate over stdio using JSON (Cursor's contract). `tddy-tools session-hook` reads the
    inbound JSON from stdin, maps the `hook_event_name` field, and emits the RPC — same fail-quiet
    contract as the Claude path (2-second reqwest timeout, exit 0 on no-op events).

### Telegram `/start-cursor`
18. A `/start-cursor <prompt>` Telegram flow is added parallel to `/start-claude`. After project →
    branch → model selection, the daemon spawns a Cursor CLI session with the user's message as the
    initial prompt (passed as a positional arg to `cursor-agent`). Resume does not replay the prompt.

## Success criteria

- A user can start, interact with, resume, signal, and delete a Cursor Agent CLI session from the
  web UI, with behavior matching the Claude CLI session feature except where explicitly noted.
- `ListSessions` shows Cursor CLI sessions with `agent = cursor-cli`, the selected model, and a
  live `activity_status` driven by `.cursor/hooks.json` events.
- A `/start-cursor` Telegram flow starts a Cursor CLI session with an optional initial prompt.
- No regressions to existing `tool` or `claude-cli` session types.

## Non-goals (out of scope for v1)

- **Sandbox mode** — no Seatbelt/cgroups jail for Cursor CLI sessions in v1 (deferred; the
  multi-select clarification explicitly excluded sandbox from v1). The `sandbox` flag is rejected
  with `failed_precondition` when `session_type = "cursor-cli"`.
- **`WaitingForInput` activity status** — Cursor's hook system has no permission-prompt
  notification equivalent in v1.
- **TDD/bugfix workflow integration** — Cursor CLI sessions have no recipe.
- **Model switching after session start.**
- **Session chaining (`previous_session_id`) for Cursor CLI sessions.**
- **LiveKit presence for Cursor CLI session processes.**

## Architecture (preliminary — to be refined in Plan mode)

### Session type discriminant
`StartSessionRequest.session_type` gains a third value, `"cursor-cli"` (alongside `"tool"` default
and `"claude-cli"`). The daemon branches on this field in `connection_service.rs`. Existing sessions
are unaffected.

### `SessionMetadata` additions
```yaml
session_type: cursor-cli    # optional; absent/empty = "tool"
model: claude-4.6-sonnet-medium-thinking   # Cursor-supported model id
```
`repo_path` stores the worktree absolute path (same convention as Claude CLI).

### Session manager
A new `CursorCliSessionManager` (sibling of `ClaudeCliSessionManager`) in `tddy-daemon`, or a
generalized `CliSessionManager` parameterized by binary name, argv builder, and hook-settings
builder. The Plan mode discussion will decide between (a) a new sibling manager or (b) refactoring
`ClaudeCliSessionManager` into a shared generic manager with per-tool adapters. ThePTY registry,
worktree creation, resume, and delete paths are reused as-is.

### Hook wiring
A new `tddy_core::build_cursor_hooks_settings(&HookCommandParams { ... })` (sibling of
`build_claude_hooks_settings`) emits the `.cursor/hooks.json` JSON. `tddy-tools session-hook` is
extended (or a sibling subcommand added) to parse Cursor's stdio JSON hook payload and map
`hook_event_name` to `SessionActivityStatus`.

### Worktree naming
Branch: `cursor-cli/<first-8-chars-of-session-uuid>`
Worktree path: `<main-repo-root>/.worktrees/cursor-cli-<first-8-chars>/`

### Web UI
`ConnectionScreen` Session type selector gains a third radio option **Cursor Agent CLI**. The Model
dropdown is populated from `ListAgents` filtered to `cursor-cli`-capable agents. `ConnectionScreen`
mounts `GhosttyTerminalGrpc` when the attached session's `agent` is `cursor-cli` (same condition as
`claude-cli`).

### Proto delta (preliminary)
No new RPCs. `StartSessionRequest.session_type` accepts the new `"cursor-cli"` value. `sandbox = true`
with `session_type = "cursor-cli"` returns `failed_precondition`.

## Risks and mitigations

- **Cursor CLI hook reliability** — Forum reports indicate Cursor CLI hook firing is less consistent
  than the IDE. *Mitigation:* fail-quiet contract; missing hooks simply mean `activity_status` stays
  on its last value. The session remains fully usable without hooks.
- **`WaitingForInput` gap** — Users coming from Claude CLI may expect the idle/permission badge.
  *Mitigation:* documented v1 gap; status remains `Running`/`ExecutingTool`/`Done`.
- **Binary discovery** — `cursor-agent` must be on `$PATH` (or resolved via a `cursor_cli:` config
  block). *Mitigation:* mirror `resolve_cursor_agent_binary` resolution used by the existing
  `CursorBackend`, with a clear error if not found.
- **Manager duplication** — A naive sibling manager duplicates ~1000 lines of `ClaudeCliSessionManager`.
  *Mitigation:* Plan mode evaluates a generic `CliSessionManager` refactor vs. a sibling, trading
  short-term duplication against refactor risk to the stable Claude path.

## References

- [Claude Code CLI session](claude-cli-session.md) — sibling feature, architectural template.
- [Connection service](../../../packages/tddy-daemon/docs/connection-service.md) — daemon RPC impl.
- [Telegram session control](telegram-session-control.md) — `/start-claude` flow template.
- [Cursor Hooks docs](https://cursor.com/docs/hooks.md) — `.cursor/hooks.json` contract.
- [Cursor CLI headless docs](https://cursor.com/docs/cli/headless) — `cursor-agent` / `agent` CLI.
