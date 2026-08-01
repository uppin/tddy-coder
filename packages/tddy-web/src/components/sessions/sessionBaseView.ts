// Which surface the session main pane shows below its top bar, and whether that session can be
// brought back. Both rules are pure functions of the `SessionEntry` so they can be pinned by unit
// tests without React, and so the pane derives them in one place instead of re-deriving liveness.
//
// PRD: docs/ft/web/inactive-session-activities.md § View selection.

import { connectionStatusForSession } from "../../utils/connectionStatusForSession";
import type { SessionEntry } from "../../gen/connection_pb";

/** The main pane's base view: a per-workflow screen, the recorded ACP transcript, or the terminal. */
export type SessionBaseViewMode = "workflow" | "activities" | "terminal";

/** True when the session has no live agent behind it. Liveness comes from
 *  `connectionStatusForSession`, so a *running* session waiting on an elicitation counts as live,
 *  while one that died still carrying the flag is dormant like any other — nobody is waiting on the
 *  operator once the process is gone. */
function isDormant(session: SessionEntry | null): boolean {
  return session !== null && connectionStatusForSession(session) === "disconnected";
}

/**
 * The base view for `session`.
 *
 * A workflow view (PR-Stack, workflow chat) wins outright: it owns its own chrome and renders from
 * persisted state, so it stays meaningful when the session is dormant. Otherwise a dormant session
 * shows its recorded activities — the Activities view replaces exactly one surface, the terminal.
 */
export function sessionBaseViewMode(
  session: SessionEntry | null,
  hasWorkflowView: boolean,
): SessionBaseViewMode {
  if (hasWorkflowView) return "workflow";
  return isDormant(session) ? "activities" : "terminal";
}

/** True when the pane should offer Resume. Keyed on liveness alone, so a dormant session gets the
 *  button in the same top-bar position whatever its base view is. */
export function canResumeSession(session: SessionEntry | null): boolean {
  return isDormant(session);
}
