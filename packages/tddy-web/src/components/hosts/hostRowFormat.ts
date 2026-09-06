/**
 * Presentation helpers for a Hosts row.
 *
 * A host's *existence* and its *reachability* are separate facts on this screen, so the formatting
 * keeps them separate too: an online host says so, and an offline one says when it was last seen
 * rather than pretending to a current reading.
 */

/** How a host's liveness reads in a row. */
export type HostLiveness = "online" | "offline";

/**
 * A last-seen stamp as a short relative phrase ("3 minutes ago").
 *
 * `nowUnixMs` is passed rather than read from the clock so a test pins a phrase instead of racing
 * real time.
 */
export function formatLastSeen(lastSeenUnixMs: bigint | number, nowUnixMs: number): string {
  // TODO(host-registry): implement
  void lastSeenUnixMs;
  void nowUnixMs;
  throw new Error("host-registry: formatLastSeen not implemented");
}
