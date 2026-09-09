/**
 * Presentation helpers for a Hosts row.
 *
 * A host's *existence* and its *reachability* are separate facts on this screen, so the formatting
 * keeps them separate too: an online host says so, and an offline one says when it was last seen
 * rather than pretending to a current reading.
 */

const MINUTE_MS = 60_000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;

/** "1 minute ago" / "3 minutes ago" — the unit is pluralised, never abbreviated to "1 minutes". */
function agoPhrase(count: number, unit: string): string {
  return `${count} ${unit}${count === 1 ? "" : "s"} ago`;
}

/**
 * A last-seen stamp as a short relative phrase ("3 minutes ago").
 *
 * `nowUnixMs` is passed rather than read from the clock so a test pins a phrase instead of racing
 * real time.
 *
 * A stamp that is not in the past reads "just now": the daemon's clock and the browser's need not
 * agree to the second, and a host last seen "in 4 seconds" is noise, not information.
 */
export function formatLastSeen(lastSeenUnixMs: bigint | number, nowUnixMs: number): string {
  // Millisecond stamps are far below 2^53, so widening the wire's int64 loses nothing.
  const elapsedMs = nowUnixMs - Number(lastSeenUnixMs);

  if (elapsedMs < MINUTE_MS) {
    return "just now";
  }
  if (elapsedMs < HOUR_MS) {
    return agoPhrase(Math.floor(elapsedMs / MINUTE_MS), "minute");
  }
  if (elapsedMs < DAY_MS) {
    return agoPhrase(Math.floor(elapsedMs / HOUR_MS), "hour");
  }
  return agoPhrase(Math.floor(elapsedMs / DAY_MS), "day");
}
