/**
 * How long the PR-Stack view waits before asking `QueryBranch` again, given how many polls in a row
 * answered exactly what the view already showed.
 *
 * Each `QueryBranch` costs the daemon a session scan, a git comparison on its blocking lane and a PR
 * lookup — a desktop window left open on a three-branch stack polled every five seconds overnight:
 * ~2,200 lookups an hour, 29k blocking-lane refusals and an 81 MB daemon log, with nobody at the
 * machine. A stack nobody is changing must not cost that.
 */

/** Interval (ms) between `QueryBranch` polls while the answers keep changing. */
export const POLL_INTERVAL_MS = 5000;

/**
 * The wait after a poll that found nothing new, given how many consecutive polls came back
 * unchanged: the base interval below three unchanged answers, then 15 s from the third, 30 s from
 * the sixth, and never more than a minute. A changed answer resets the count, so the view is back
 * to the base interval as soon as something moves.
 */
export function nextQueryBranchDelayMs(unchangedPolls: number): number {
  if (unchangedPolls >= 12) return 60_000;
  if (unchangedPolls >= 6) return 30_000;
  if (unchangedPolls >= 3) return 15_000;
  return POLL_INTERVAL_MS;
}
