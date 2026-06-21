import type { SessionEntry } from "../gen/connection_pb";

/**
 * Derives a human-readable display label for a session in the drawer.
 *
 * Priority:
 * 1. Basename of `repoPath` (when non-empty and yields a non-empty segment).
 * 2. `workflowGoal` (when non-empty).
 * 3. First 8 characters of `sessionId` as a last-resort fallback.
 */
export function sessionDrawerLabel(session: SessionEntry): string {
  const trimmedRepo = session.repoPath.trim();
  if (trimmedRepo !== "" && trimmedRepo !== "/") {
    const basename = trimmedRepo.split("/").filter(Boolean).at(-1);
    if (basename && basename !== "") {
      return basename;
    }
  }

  if (session.workflowGoal.trim() !== "") {
    return session.workflowGoal.trim();
  }

  return session.sessionId.slice(0, 8);
}
