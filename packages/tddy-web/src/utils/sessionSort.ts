import type { SessionEntry } from "../gen/connection_pb";

/**
 * Parse `createdAt` as ISO-8601 (or other strings `Date` understands).
 * Returns `NaN` when the value is not a valid date — callers use `sessionId` for ordering.
 */
function createdAtMs(createdAt: string): number {
  const ms = Date.parse(createdAt);
  return Number.isFinite(ms) ? ms : Number.NaN;
}

/**
 * Compare two sessions for display order:
 * 1. Active (`isActive === true`) before inactive.
 * 2. Within the same activity group, newer `createdAt` first (descending by time).
 * 3. If timestamps are equal or either side fails to parse, order by `sessionId` ascending (stable, deterministic).
 */
function compareSessionsForDisplay(a: SessionEntry, b: SessionEntry): number {
  if (a.isActive !== b.isActive) {
    return a.isActive ? -1 : 1;
  }
  const ta = createdAtMs(a.createdAt);
  const tb = createdAtMs(b.createdAt);
  if (Number.isFinite(ta) && Number.isFinite(tb) && ta !== tb) {
    return tb - ta;
  }
  return a.sessionId.localeCompare(b.sessionId);
}

/**
 * Orders sessions for the Connection screen: active first, then by recency (`createdAt`
 * descending). Sorting is pure and deterministic for the same input array.
 *
 * Unparsable `createdAt` values are treated as tied on time; order is then by `sessionId`
 * (lexicographic ascending), per PRD.
 */
export function sortSessionsForDisplay(sessions: SessionEntry[]): SessionEntry[] {
  const dev = import.meta.env?.DEV === true;
  if (dev) {
    console.debug(
      "[tddy-web] sortSessionsForDisplay: input",
      JSON.stringify({
        count: sessions.length,
        sessionIds: sessions.map((s) => s.sessionId),
        activeCount: sessions.filter((s) => s.isActive).length,
      }),
    );
  }
  const out = [...sessions].sort(compareSessionsForDisplay);
  if (dev) {
    console.info(
      "[tddy-web] sortSessionsForDisplay: ordered sessionIds",
      out.map((s) => s.sessionId).join(", "),
    );
  }
  return out;
}
