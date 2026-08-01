import type { SessionEntry } from "../gen/connection_pb";

/** Display status token for a session. */
export type ConnectionStatus = "connected" | "disconnected" | "needs-input";

/**
 * Maps session proto fields to a display status token.
 *
 * - `"needs-input"` — session is active AND has a pending elicitation.
 * - `"connected"` — session is active with no pending elicitation.
 * - `"disconnected"` — session is not active, whatever its elicitation flag says.
 *
 * `pendingElicitation` is persisted state that is **not cleared when the agent dies**, so a dead
 * session can still carry it. "Needs input" claims someone is waiting on the operator, and a dead
 * session is not — answering it would reach nothing. Liveness therefore decides first, and the
 * elicitation flag only refines a session that is genuinely alive. This keeps a crashed-mid-
 * elicitation session readable as what it is: dormant, showing its recorded activities, with a
 * Resume button (see docs/ft/web/inactive-session-activities.md).
 */
export function connectionStatusForSession(session: SessionEntry): ConnectionStatus {
  if (!session.isActive) {
    return "disconnected";
  }
  return session.pendingElicitation ? "needs-input" : "connected";
}
