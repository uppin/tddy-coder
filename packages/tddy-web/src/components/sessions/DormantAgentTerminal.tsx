import type React from "react";
import type { TerminalClient } from "./sessionRuntimeProps";

/**
 * The Agent tab of a session whose agent is not running: what the agent terminal last printed,
 * read-only and in the terminal's history colour scheme (`HISTORY_THEME`, `HISTORY_FILTER`), with a
 * toast that says the session is not running and offers Resume.
 *
 * The output comes from `GetTerminalHistory` for the `main` terminal, which the daemon answers from
 * the session's recording once the process has exited. Keystrokes go nowhere. A session with no
 * recording says "No recorded output".
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-02-dormant-agent-tab.md (`#session-tabs 6/7`)
 */
export interface DormantAgentTerminalProps {
  sessionId: string;
  sessionToken: string;
  /** The owning daemon's terminal client — where the recorded history is read. */
  terminalClient?: TerminalClient | null;
  /** Resume the session. A rejection is shown in the toast and the greyed view stays. */
  onResume: () => Promise<void>;
}

export function DormantAgentTerminal(_props: DormantAgentTerminalProps): React.ReactElement | null {
  // TODO(dormant-agent-tab): implement
  return null;
}
