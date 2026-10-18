import { Code, ConnectError } from "@connectrpc/connect";

/**
 * Reopening a server stream that ended under its follower — the Code pane's `CodeSession` and the
 * session header's `WatchCodeIndex` share one policy:
 *
 * - The waits double from 1 s up to 30 s between attempts.
 * - They start from 1 s again only once a stream has **proved stable**: it delivered and stayed open
 *   for at least `STABLE_STREAM_MS`. A host that accepts a stream, says something and drops it (an
 *   index daemon crashing on start) is therefore backed off like one that refuses at once, rather
 *   than asked again every second — each attempt can spawn a process on the host.
 * - A refusal (the host answered, and asking again cannot change its answer) is not reopened.
 */

/** The first wait before reopening a stream that ended, in milliseconds. */
const FIRST_REOPEN_DELAY_MS = 1_000;
/** The longest wait between two reopening attempts, in milliseconds. */
const LONGEST_REOPEN_DELAY_MS = 30_000;
/** How long a stream that delivered must stay open before the waits start from 1 s again. */
export const STABLE_STREAM_MS = 10_000;

/** How long to wait before reopening attempt `attempt` (zero-based): doubling, capped. */
export const reopenDelayMs = (attempt: number) =>
  Math.min(FIRST_REOPEN_DELAY_MS * 2 ** attempt, LONGEST_REOPEN_DELAY_MS);

/**
 * Refusals: the host answered, and asking again cannot change its answer, so the stream is not
 * reopened. Everything else (the transport dropped, the daemon restarted) is reopened.
 */
const REFUSALS: ReadonlySet<Code> = new Set([
  Code.InvalidArgument,
  Code.NotFound,
  Code.PermissionDenied,
  Code.Unauthenticated,
  Code.FailedPrecondition,
  Code.Unimplemented,
]);

/** How a stream ended: why, and whether the host refused it. */
export type StreamEnd = { reason: string; refused: boolean };

/** Why a stream failed: its own words for an RPC error, without the code prefix. */
export function streamEndOf(err: unknown): StreamEnd {
  if (err instanceof ConnectError) return { reason: err.rawMessage, refused: REFUSALS.has(err.code) };
  return {
    reason: err instanceof Error ? err.message : String(err),
    refused: false,
  };
}

/**
 * The reopening schedule of one follower: tell it when a stream opens and delivers, ask it to
 * reopen once one ends, and `stop` it when the follower lets go.
 */
export class StreamReopener {
  private attempt = 0;
  private openedAt = 0;
  private delivered = false;
  private timer: ReturnType<typeof setTimeout> | null = null;

  /** A stream opened now. */
  opened(): void {
    this.openedAt = Date.now();
    this.delivered = false;
  }

  /** The stream open now delivered something. */
  heard(): void {
    this.delivered = true;
  }

  /** The stream ended: run `reopen` after the wait the schedule is at. */
  reopenLater(reopen: () => void): void {
    if (this.delivered && Date.now() - this.openedAt >= STABLE_STREAM_MS) this.attempt = 0;
    this.timer = setTimeout(() => {
      this.timer = null;
      reopen();
    }, reopenDelayMs(this.attempt));
    this.attempt += 1;
  }

  /** The follower let go: nothing waiting is reopened. */
  stop(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
}
