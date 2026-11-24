import type { SessionEntry } from "tddy-client-web/gen/session_pb";

/**
 * Parse an RFC 3339 timestamp (`createdAt`, `updatedAt`) as ISO-8601 (or other strings `Date` understands).
 * Returns `NaN` when the value is not a valid date — callers use `sessionId` for ordering.
 */
function timestampMs(value: string): number {
  const ms = Date.parse(value);
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
  const ta = timestampMs(a.createdAt);
  const tb = timestampMs(b.createdAt);
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
  return [...sessions].sort(compareSessionsForDisplay);
}

/**
 * Compare two sessions purely by creation time (no active-first grouping):
 * 1. Newer `createdAt` first (descending by time).
 * 2. If timestamps are equal or either side fails to parse, order by `sessionId` ascending.
 */
function compareSessionsByCreation(a: SessionEntry, b: SessionEntry): number {
  const ta = timestampMs(a.createdAt);
  const tb = timestampMs(b.createdAt);
  if (Number.isFinite(ta) && Number.isFinite(tb) && ta !== tb) {
    return tb - ta;
  }
  return a.sessionId.localeCompare(b.sessionId);
}

/**
 * Orders sessions purely by creation time, newest first — no active-first grouping.
 *
 * Tie-break: `sessionId` ascending (stable, deterministic).
 * Unparsable `createdAt` values are treated as tied on time.
 */
export function sortSessionsByCreation(sessions: SessionEntry[]): SessionEntry[] {
  return [...sessions].sort(compareSessionsByCreation);
}

/**
 * When the session was last touched: the later of its agent's last observed activity
 * (`lastActivity.atUnixMs`) and its last metadata update (`updatedAt`, which the daemon bumps on a
 * hook-reported status change such as a session ending). `NaN` when neither is known.
 */
export function sessionLastTouchedMs(session: SessionEntry): number {
  const activityMs = session.lastActivity ? Number(session.lastActivity.atUnixMs) : Number.NaN;
  const updatedMs = timestampMs(session.updatedAt);
  const known = [activityMs, updatedMs].filter((ms) => Number.isFinite(ms) && ms > 0);
  return known.length > 0 ? Math.max(...known) : Number.NaN;
}

/**
 * Orders sessions by {@link sessionLastTouchedMs}, most recent first — how the drawer lists the
 * sessions under **More**, so the one that stopped most recently leads.
 *
 * Tie-break: `sessionId` ascending. A session with no known time sorts after every one with a time.
 */
export function sortSessionsByLastTouched(sessions: SessionEntry[]): SessionEntry[] {
  return [...sessions].sort((a, b) => {
    const ta = sessionLastTouchedMs(a);
    const tb = sessionLastTouchedMs(b);
    const knownA = Number.isFinite(ta);
    const knownB = Number.isFinite(tb);
    if (knownA !== knownB) return knownA ? -1 : 1;
    if (knownA && ta !== tb) return tb - ta;
    return a.sessionId.localeCompare(b.sessionId);
  });
}
