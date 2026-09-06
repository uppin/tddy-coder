/**
 * The read loop behind `useHostStats`, as a plain function so that closing it is observable.
 *
 * The loop lives here rather than in the hook because letting go of a stream is the part that
 * matters and the part a component test cannot see. A `for await` cannot do it: while it is parked
 * awaiting the next frame there is no way to reach a `break`, so a host that subscribes and never
 * reports is never released — a screenful of quiet hosts leaks a subscription each. Iterating by
 * hand keeps the iterator addressable, and `unsubscribe()` closes it directly.
 *
 * The split is the one `useHasCapability`/`hasCapability` already uses: the logic is a pure function
 * with unit tests, the hook is a thin wrapper.
 *
 * PRD: `docs/ft/web/1-WIP/PRD-2026-09-06-telemetry-fanout.md` (AC-6)
 */

/**
 * The part of a `HostStatsEvent` this loop passes on — structurally satisfied by the generated
 * message, so the hook hands over the wire type unchanged and tests can hand over a literal.
 */
export interface HostStatsEventLike {
  cpu?: { perCorePercent: number[] };
  disk?: { availableBytes: bigint; totalBytes: bigint; projectDir: string };
}

export interface HostStatsSubscription {
  /** Stop reading and close the stream, whether or not it has emitted. Safe to call twice. */
  unsubscribe: () => void;
}

/**
 * Read `open()`'s stream, handing each event to `onEvent` until the caller unsubscribes.
 *
 * A failed feed is swallowed: the caller keeps the last reading it was given rather than a
 * fabricated one, and the stream is still released. An error thrown by `onEvent` is deliberately
 * outside that guard — a bug in the caller's handler must not disappear into the feed's failure
 * path.
 *
 * @param open opens the stream; called once, on subscribe.
 * @param onEvent receives every event, in the order the daemon sent them, until unsubscribed.
 */
export function subscribeHostStats(
  open: () => AsyncIterable<HostStatsEventLike>,
  onEvent: (event: HostStatsEventLike) => void,
): HostStatsSubscription {
  let unsubscribed = false;
  let stream: AsyncIterator<HostStatsEventLike> | null = null;

  /** Release the stream. A second call has nothing left to release. */
  const close = () => {
    const released = stream;
    stream = null;
    // A stream that objects to being closed is gone either way; there is nothing to recover.
    void released?.return?.().catch(() => undefined);
  };

  const feedFailed = (error: unknown) => {
    console.debug("[hostStatsSubscription] host stats stream ended", error);
  };

  void (async () => {
    try {
      stream = open()[Symbol.asyncIterator]();
    } catch (error) {
      feedFailed(error);
      return;
    }
    const reading = stream;
    try {
      while (!unsubscribed) {
        let frame: IteratorResult<HostStatsEventLike>;
        try {
          frame = await reading.next();
        } catch (error) {
          feedFailed(error);
          return;
        }
        // The caller may have let go while that frame was in flight; `unsubscribe` has already
        // closed the stream, and delivering now would update a component that stopped listening.
        if (unsubscribed || frame.done) return;
        onEvent(frame.value);
      }
    } finally {
      close();
    }
  })();

  return {
    unsubscribe: () => {
      if (unsubscribed) return;
      unsubscribed = true;
      // Closing here, rather than letting the loop notice, is the whole point: the loop may be
      // parked on a first frame that never comes.
      close();
    },
  };
}
