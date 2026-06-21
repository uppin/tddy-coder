import type { SessionEntry } from "../gen/connection_pb";

/** Display status token for a session. */
export type ConnectionStatus = "connected" | "disconnected" | "needs-input";

/**
 * Maps session proto fields to a display status token.
 *
 * - `"needs-input"` — `pendingElicitation` is true (takes precedence over `isActive`).
 * - `"connected"` — session is active and has no pending elicitation.
 * - `"disconnected"` — session is inactive and has no pending elicitation.
 */
export function connectionStatusForSession(session: SessionEntry): ConnectionStatus {
  if (session.pendingElicitation) {
    return "needs-input";
  }
  return session.isActive ? "connected" : "disconnected";
}
