import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import { connectionStatusForSession } from "./connectionStatusForSession";

/**
 * The sessions `session` started that a close could take with it: rows whose
 * `orchestratorSessionId` is `session` and that are still active or still hold a worktree. A child
 * that has ended and holds nothing has nothing left to close (docs/ft/web/session-drawer.md §
 * Closing a session).
 */
export function closableChildren(
  session: SessionEntry,
  sessions: ReadonlyArray<SessionEntry>,
): SessionEntry[] {
  return sessions.filter(
    (candidate) =>
      candidate.sessionId !== session.sessionId &&
      candidate.orchestratorSessionId === session.sessionId &&
      (connectionStatusForSession(candidate) !== "disconnected" || candidate.holdsWorktree),
  );
}
