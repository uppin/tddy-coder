import type { SessionEntry } from "../gen/connection_pb";

/**
 * Returns sessions that are referenced as `orchestratorSessionId` by at least one other session
 * in the list — i.e. sessions currently acting as PR-stack orchestrators.
 *
 * Useful for populating the parent-picker `<select>` in the new-session screen.
 */
export function stackParentCandidates(sessions: SessionEntry[]): SessionEntry[] {
  // Collect all orchestratorSessionIds referenced by child sessions.
  const referencedIds = new Set<string>();
  for (const s of sessions) {
    if (s.orchestratorSessionId.length > 0) {
      referencedIds.add(s.orchestratorSessionId);
    }
  }
  if (referencedIds.size === 0) return [];

  // Return sessions whose sessionId is in the referenced set (deduped by Set lookup).
  const seen = new Set<string>();
  const result: SessionEntry[] = [];
  for (const s of sessions) {
    if (referencedIds.has(s.sessionId) && !seen.has(s.sessionId)) {
      seen.add(s.sessionId);
      result.push(s);
    }
  }
  return result;
}
