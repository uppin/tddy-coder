/**
 * Claude Code CLI model list and session type helpers.
 *
 * `CreateSessionPane` no longer uses `CLAUDE_CLI_MODELS`: it sources the claude-cli catalog from the
 * daemon via the `ListAgentModels` RPC (agent `"claude-cli"`, backed by tddy-core). See
 * docs/ft/web/tool-session-model-selection.md. `CLAUDE_CLI_MODELS` is retained only for the legacy
 * `ConnectionScreen` inline form, which is out of scope for that change.
 */

export interface ClaudeCliModel {
  /** Model identifier sent to the daemon (e.g. "claude-opus-4-8"). */
  id: string;
  /** Human-readable label for UI display (e.g. "Claude Opus 4.8"). */
  label: string;
}

/** Ordered list of Claude CLI models available for selection (legacy ConnectionScreen only). */
export const CLAUDE_CLI_MODELS: ClaudeCliModel[] = [
  { id: "claude-opus-4-8", label: "Claude Opus 4.8" },
  { id: "claude-sonnet-4-6", label: "Claude Sonnet 4.6" },
  { id: "claude-haiku-4-5-20251001", label: "Claude Haiku 4.5" },
];

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
