# Changeset: Claude Code CLI Session Type

**Feature**: Claude Code CLI session — persistent `claude` CLI session managed by `tddy-daemon`, with PTY terminal access via the web UI.  
**PRD**: [`docs/ft/daemon/claude-cli-session.md`](../../ft/daemon/claude-cli-session.md)  
**Date**: 2026-06-06

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset

### tddy-service

- [ ] **`connection.proto`**: Add `session_type` (field 7) and `model` (field 8) to `StartSessionRequest`
- [ ] **`connection.proto`**: Add `StreamSessionTerminalIO` RPC, `SessionTerminalInput` message, `SessionTerminalOutput` message

### tddy-core

- [ ] **`session_metadata.rs`**: Add `session_type: Option<String>` and `model: Option<String>` fields to `SessionMetadata`; update `write_initial_tool_session_metadata()` to accept and persist these; update YAML serde accordingly
- [ ] **`session_metadata.rs`**: Update `write_initial_tool_session_metadata` signature to accept session type + model; add `write_initial_claude_cli_session_metadata()` convenience wrapper

### tddy-daemon

- [ ] **`packages/tddy-daemon/src/claude_cli_session.rs`** (new): `ClaudeCliSessionManager` — PTY spawn, monitor task, resume, connect, PTY registry (`DashMap<String, Arc<PtyHandle>>`)
  - `PtyHandle` struct: master fd, child PID, worktree path, model
  - `start_claude_cli_session(session_id, project_id, model, sessions_base) -> Result<PtyHandle>`
  - `resume_claude_cli_session(session_id, sessions_base) -> Result<PtyHandle>`
  - `connect_claude_cli_session(session_id) -> Option<Arc<PtyHandle>>`
  - Background monitor: `waitpid` loop → on exit mark `.session.yaml` inactive, remove from registry
- [ ] **`connection_service.rs`**: `start_session` branch on `session_type == "claude-cli"` — call `ClaudeCliSessionManager::start_claude_cli_session`, create worktree via `create_worktree_with_retry` (branch `claude-cli/<short-id>`, path `<repo>/.worktrees/claude-cli-<short-id>/`), write session metadata, return `StartSessionResponse` with empty LiveKit fields
- [ ] **`connection_service.rs`**: `connect_session` — detect `session_type == "claude-cli"` from metadata, call `ClaudeCliSessionManager::connect_claude_cli_session`, return `ConnectSessionResponse` with empty LiveKit fields and a sentinel indicating gRPC terminal mode
- [ ] **`connection_service.rs`**: `resume_session` — detect `session_type == "claude-cli"`, call `ClaudeCliSessionManager::resume_claude_cli_session`, update PID in `.session.yaml`, return `ResumeSessionResponse` with empty LiveKit fields
- [ ] **`connection_service.rs`**: `signal_session` — detect `session_type == "claude-cli"`, send signal to PID from registry (or from `.session.yaml` if inactive)
- [ ] **`connection_service.rs`**: `delete_session` — detect `session_type == "claude-cli"`, SIGTERM PTY process, wait for exit, call `remove_worktree()` for the session worktree, remove session directory
- [ ] **`connection_service.rs`**: `stream_session_terminal_io` — look up `PtyHandle` by session ID, spawn write task (client stream → PTY master fd) and read task (PTY master fd → response stream); handle resize via OSC `\x1b]resize;{cols};{rows}\x07` on the input side (same convention as VirtualTui)
- [ ] **`list_sessions` enrichment**: For `session_type == "claude-cli"` sessions, populate `agent = "claude-cli"`, `model` from metadata, leave `workflow_goal`, `workflow_state`, `elapsed_display` as empty (rendered as `—` by web)
- [ ] **`config.rs`**: Add optional `claude_cli.binary_path: String` (default: `"claude"` from PATH) for overriding the `claude` binary location

### tddy-web

- [ ] **`ConnectionScreen.tsx`**: Add **Session type** selector (radio group or tab strip) per project row: `"managed"` (default) | `"claude-cli"`. Store in `ProjectSessionForm.sessionType`.
- [ ] **`ConnectionScreen.tsx`**: When `sessionType == "claude-cli"`: hide Tool, Backend, Recipe controls; show Model `<select>` populated from `claudeCliModels` constant.
- [ ] **`ConnectionScreen.tsx`**: Pass `session_type` and `model` in `StartSessionRequest`; pass empty `tool_path`, `agent`, `recipe`.
- [ ] **`ConnectionScreen.tsx`**: On connect/resume for a session where `agent == "claude-cli"` (from `ListSessions`): mount `GhosttyTerminalGrpc` instead of `GhosttyTerminalLiveKit`.
- [ ] **`components/GhosttyTerminalGrpc.tsx`** (new): React component wrapping `GhosttyTerminal` with a gRPC `StreamSessionTerminalIO` bidi stream. Props: `sessionToken`, `sessionId`, `grpcClient`, `fontSize?`, `connectionChromePlacement?`. Follows same connection-chrome pattern as `GhosttyTerminalLiveKit` (status dot, disconnect, terminate menu).
- [ ] **`constants/claudeCliModels.ts`** (new): Static array of `{ id, label }` for model dropdown:
  - `{ id: "claude-opus-4-8", label: "Claude Opus 4" }`
  - `{ id: "claude-sonnet-4-6", label: "Claude Sonnet 4.5" }`
  - `{ id: "claude-haiku-4-5-20251001", label: "Claude Haiku 4.5" }`
- [ ] **Session table**: No changes needed — existing `agent` and `model` columns already render `—` for empty strings; `agent = "claude-cli"` will render as-is.

### Tests

- [ ] **`tddy-daemon` unit tests**: `ClaudeCliSessionManager` — PTY spawn round-trip with a short-lived process (e.g. `echo hello`), monitor task marks session inactive on exit
- [ ] **`tddy-daemon` integration test**: `claude_cli_session_acceptance` — start session, stream I/O, process exits, session marked inactive, resume relaunches in same worktree
- [ ] **`tddy-web` Bun tests**: `claudeCliModels` constant shape; `ConnectionScreen` session-type toggle shows/hides correct controls (unit-level)
- [ ] **`tddy-web` Cypress component test**: `GhosttyTerminalGrpc.cy.tsx` — mock gRPC stream, assert terminal renders data, resize OSC sent on container resize

### Documentation

- [ ] Update `docs/ft/daemon/changelog.md` with Claude Code CLI session entry
- [ ] Update `docs/ft/web/web-terminal.md` — add Claude CLI session type to session table columns reference; note `agent == "claude-cli"` routes to `GhosttyTerminalGrpc`
- [ ] Update `packages/tddy-daemon/docs/connection-service.md` — document `StreamSessionTerminalIO`, `ClaudeCliSessionManager`, session-type branching
- [ ] Update `packages/tddy-service/docs/changesets.md`
- [ ] Update `packages/tddy-daemon/docs/changesets.md`
- [ ] Update `packages/tddy-core/docs/changesets.md`
- [ ] Update `packages/tddy-web/docs/changesets.md`

## Package scope

`tddy-service`, `tddy-core`, `tddy-daemon`, `tddy-web`, docs

## Notes

- PTY reads/writes on the daemon side must use non-blocking I/O or a dedicated thread (not tokio task directly on raw fd) to avoid blocking the async runtime.
- The `claude` binary must be on PATH for the OS user that the daemon spawns as; document this in operator setup.
- `StreamSessionTerminalIO` auth: `session_token` is validated on the **first** `SessionTerminalInput` message only. Subsequent messages skip token validation (connection is already established).
- Resume does not attempt to restore terminal scrollback — `claude` starts a fresh interactive session in the same worktree. The worktree's file state is preserved.
- Worktree cleanup on `DeleteSession` uses the existing `remove_worktree()` from `tddy-core`; no new mechanism needed.
