/**
 * Session-type predicates for the CLI-backed agents.
 *
 * There is deliberately no model list here. The Claude and Cursor CLI catalogs live in tddy-core
 * (`backend::claude_cli_models`, `backend::cursor_cli_models`) and reach the web only over the
 * `ListAgentModels` RPC — see `useAgentModels` and docs/ft/web/tool-session-model-selection.md. A
 * second copy in the frontend would drift from the ids the daemon actually passes to `--model`.
 */

/**
 * Returns `true` when the `agent` field from a session entry indicates that
 * the session is a raw Claude Code CLI session (no LiveKit, terminal I/O via
 * `StreamSessionTerminalIO`).
 */
export function isClaudeCliSession(agent: string): boolean {
  return agent === "claude-cli";
}

/** Raw Cursor Agent CLI session — terminal I/O via `StreamSessionTerminalIO`. */
export function isCursorCliSession(agent: string): boolean {
  return agent === "cursor-cli";
}

/** Claude or Cursor CLI session (PTY terminal, no LiveKit for session process). */
export function isCliTerminalSession(agent: string): boolean {
  return isClaudeCliSession(agent) || isCursorCliSession(agent);
}
