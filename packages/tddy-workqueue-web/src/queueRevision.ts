/**
 * Which copy of a work queue the view keeps. Every copy the daemon hands out — `WatchWorkQueue`'s
 * snapshots, `GetWorkQueue`, and the answer of every mutation — is the whole queue, numbered by its
 * `revision`, which only grows. The view keeps the newest it has seen, so a snapshot that arrives
 * after a newer answer (or twice, after a reconnect) changes nothing.
 */

/** The revision a `work_queue_json` carries; `0` when it carries none (or is not a queue). */
export function revisionOf(_workQueueJson: string): number {
  // TODO(red): implemented by /green.
  throw new Error("revisionOf is not implemented");
}

/**
 * Whether a copy numbered `incoming` replaces the one held at `held` (`null` while nothing is held).
 * A copy numbered `0` predates numbering — an answer from a daemon that numbers nothing — and is
 * taken as it comes, since nothing orders it.
 */
export function supersedes(_held: number | null, _incoming: number): boolean {
  // TODO(red): implemented by /green.
  throw new Error("supersedes is not implemented");
}
