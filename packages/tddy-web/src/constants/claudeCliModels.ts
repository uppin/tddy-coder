/**
 * Claude Code CLI model list and session type helpers.
 *
 * PRD: docs/ft/daemon/claude-cli-session.md
 *
 * `CLAUDE_CLI_MODELS` enumerates the Claude model identifiers that the daemon
 * can pass as `--model` when spawning a `claude` CLI subprocess.  Each entry
 * has an `id` (the value sent in the `StartSession` RPC) and a human-readable
 * `label` for display in the UI.
 */

export interface ClaudeCliModel {
  /** Model identifier sent to the daemon (e.g. "claude-opus-4-8"). */
  id: string;
  /** Human-readable label for UI display (e.g. "Claude Opus 4.8"). */
  label: string;
}

/** Ordered list of Claude CLI models available for selection. */
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
