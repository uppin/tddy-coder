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

## Non-goals (out of scope)

- TDD/bugfix workflow integration — Claude Code CLI sessions have no recipe.
- Telegram elicitation — not routed through the existing elicitation mechanism.
- Model switching after session start.
- Session chaining (`previous_session_id`) for Claude Code CLI sessions.
- LiveKit presence for Claude Code CLI session processes (no `--livekit-*` args passed).

## Architecture

### Session type discriminant

`StartSessionRequest` gains a `session_type` string field (`"tool"` default, `"claude-cli"` new). The daemon branches on this field in `connection_service.rs`. Existing sessions are unaffected (absent or empty `session_type` = `"tool"`).

### `SessionMetadata` additions

```yaml
session_type: claude-cli    # new optional field; absent/empty = "tool"
model: claude-opus-4-8      # new optional field; stored for resume
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

## Proto delta (connection.proto)

```proto
message StartSessionRequest {
  // ... existing fields ...
  string session_type = 7;  // "tool" (default) or "claude-cli"
  string model        = 8;  // model id for claude-cli sessions
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
```

`StartSessionResponse`, `ConnectSessionResponse`, and `ResumeSessionResponse` are unchanged — LiveKit fields are returned as empty strings for Claude CLI sessions. The web client detects `agent == "claude-cli"` from `ListSessions` to decide which terminal component to mount.

## See also

- [Web terminal](../web/web-terminal.md) — GhosttyTerminal, connection chrome, session table
- [Connection service](../../../packages/tddy-daemon/docs/connection-service.md) — daemon RPC implementation
- [Worktrees](../web/worktrees.md) — worktree lifecycle, `create_worktree_with_retry`
