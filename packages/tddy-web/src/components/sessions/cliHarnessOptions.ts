import type { SessionType } from "./createSessionRequest";

/** The session types the Create pane's toggle offers, in order, with each button's label. */
export const SESSION_TYPE_OPTIONS: { id: SessionType; label: string }[] = [
  { id: "tool", label: "Tool" },
  { id: "claude-cli", label: "Claude CLI" },
  { id: "cursor-cli", label: "Cursor CLI" },
  { id: "opencode-cli", label: "OpenCode" },
  { id: "muse-cli", label: "Muse Code" },
  { id: "warp-cli", label: "Warp" },
];

/**
 * The CLI harnesses a terminal tab can run, in toggle order. Every session type but `tool`: a tool
 * session's terminals are served by its tddy-coder participant, which runs bash only.
 */
export const CLI_HARNESS_OPTIONS = SESSION_TYPE_OPTIONS.filter(({ id }) => id !== "tool");

/** The kind of a login-shell terminal. */
export const BASH_KIND = "bash";

/** The name a terminal of `kind` is shown by: a harness's toggle label, or "Bash". */
export function terminalKindLabel(kind: string): string {
  if (kind === BASH_KIND) return "Bash";
  return SESSION_TYPE_OPTIONS.find(({ id }) => id === kind)?.label ?? kind;
}
