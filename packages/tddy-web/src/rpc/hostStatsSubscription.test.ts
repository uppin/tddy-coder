/**
 * Unit tests for the host-stats subscription — the loop behind `useHostStats`, extracted so that
 * closing it is observable.
 *
 * Teardown is the reason this is a plain function rather than a hook. A component test cannot see
 * it: `createRouterTransport` (the in-memory testkit's transport) propagates neither an abort nor a
 * consumer's `break` to the server handler, so no fake backend can count a subscription closing.
 * A hand-rolled async iterable can — `return()` is called on it exactly when the consumer lets go —
 * which is what every test below turns on.
 *
 * The house split is the one `useHasCapability` uses: the predicate is pure and unit-tested, the
 * hook is a thin wrapper.
 *
 * PRD: `docs/ft/web/1-WIP/PRD-2026-09-06-telemetry-fanout.md` (AC-6)
 */

import { describe, it, expect } from "bun:test";
import { subscribeHostStats, type HostStatsEventLike } from "./hostStatsSubscription";

/**
 * A stream that hands over `events`, then stays open the way the daemon's feed does.
 *
 * Hand-rolled rather than an `async function*` on purpose. An async generator parked on an `await`
 * queues a `return()` behind the pending `next()` and never processes it, so its `finally` could
 * not run and no implementation could pass these tests. A plain iterator answers `return()` the
 * moment the consumer calls it — which is the contract under test.
 */
function aStreamOf(...events: HostStatsEventLike[]) {
  let closed = false;
  let handedOver = 0;
  const stream = {
    [Symbol.asyncIterator]() {
      return {
        next: async () => {
          if (handedOver < events.length) {
            return { value: events[handedOver++], done: false as const };
          }
          // Open, with nothing more to say — the daemon between readings.
          return new Promise<never>(() => undefined);
        },
        return: async () => {
          closed = true;
          return { value: undefined, done: true as const };
        },
      };
    },
  };
  return { stream, wasClosed: () => closed };
}

/**
 * A stream shaped exactly as a Connect client's: an iterator with **`next` and nothing else**.
 *
 * `@connectrpc/connect` wraps every server-stream in `{ [Symbol.asyncIterator]: () => ({ next }) }`
 * — its own comment reads "Create a new iterable to omit throw/return" — so in production there is
 * no `return()` to call and releasing the iterator cannot end anything. Only the call's abort signal
 * can. A fake that offers `return()` would let a subscription that never cancels look correct.
 */
function aConnectShapedStream(...events: HostStatsEventLike[]) {
  let cancelled = false;
  let handedOver = 0;
  const open = (signal: AbortSignal): AsyncIterable<HostStatsEventLike> => ({
    [Symbol.asyncIterator]: () => ({
      next: () => {
        if (handedOver < events.length) {
          return Promise.resolve({ value: events[handedOver++], done: false as const });
        }
        // Open, with nothing more to say — settled only by the caller giving up.
        return new Promise<IteratorResult<HostStatsEventLike>>((_resolve, reject) => {
          signal.addEventListener("abort", () => {
            cancelled = true;
            reject(new DOMException("The operation was aborted.", "AbortError"));
          });
        });
      },
    }),
  });
  return { open, wasCancelled: () => cancelled };
}

/** A stream that ends of its own accord after `events` — the daemon completing the feed. */
function aStreamThatEndsAfter(...events: HostStatsEventLike[]) {
  let closed = false;
  let handedOver = 0;
  const stream = {
    [Symbol.asyncIterator]() {
      return {
        next: async () =>
          handedOver < events.length
            ? { value: events[handedOver++], done: false as const }
            : { value: undefined, done: true as const },
        return: async () => {
          closed = true;
          return { value: undefined, done: true as const };
        },
      };
    },
  };
  return { stream, wasClosed: () => closed };
}

/** A transport that refuses to open the stream at all. */
function aStreamThatCannotOpen() {
  return () => {
    throw new Error("no transport reaches this host");
  };
}

/** A stream that opens and never emits — a host subscribed but not yet reporting. */
function aSilentStream() {
  return aStreamOf();
}

/** A stream that fails after handing over `events`. */
function aStreamThatFailsAfter(...events: HostStatsEventLike[]) {
  let closed = false;
  let handedOver = 0;
  const stream = {
    [Symbol.asyncIterator]() {
      return {
        next: async () => {
          if (handedOver < events.length) {
            return { value: events[handedOver++], done: false as const };
          }
          throw new Error("daemon dropped the feed");
        },
        return: async () => {
          closed = true;
          return { value: undefined, done: true as const };
        },
      };
    },
  };
  return { stream, wasClosed: () => closed };
}

const A_READING: HostStatsEventLike = {
  cpu: { perCorePercent: [12, 48] },
  disk: { availableBytes: 42_100_000_000n, totalBytes: 100_000_000_000n, projectDir: "/repos" },
};

const A_LATER_READING: HostStatsEventLike = {
  cpu: { perCorePercent: [90, 90] },
  disk: A_READING.disk,
};

/** Let the subscription's own microtasks run. */
function settle() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

describe("host stats subscription", () => {
  it("hands every streamed reading to the caller in order", async () => {
    // Given a feed carrying two readings
    const feed = aStreamOf(A_READING, A_LATER_READING);
    const seen: HostStatsEventLike[] = [];

    // When the caller subscribes
    const subscription = subscribeHostStats(() => feed.stream, (event) => seen.push(event));
    await settle();

    // Then it saw both, newest last
    expect(seen).toEqual([A_READING, A_LATER_READING]);
    subscription.unsubscribe();
  });

  it("closes the stream when the caller unsubscribes", async () => {
    // Given a live subscription that has already received a reading
    const feed = aStreamOf(A_READING);
    const subscription = subscribeHostStats(() => feed.stream, () => {});
    await settle();

    // When the caller lets go
    subscription.unsubscribe();
    await settle();

    // Then the stream is closed, not merely ignored
    expect(feed.wasClosed()).toBe(true);
  });

  it("closes a stream that has never emitted", async () => {
    // Given a host that is subscribed but has not reported yet
    const feed = aSilentStream();
    const subscription = subscribeHostStats(() => feed.stream, () => {});
    await settle();

    // When the caller lets go before any reading has arrived
    subscription.unsubscribe();
    await settle();

    // Then that stream is closed too — waiting on a first frame that may never come is exactly how
    // a screenful of quiet hosts would leak a subscription each.
    expect(feed.wasClosed()).toBe(true);
  });

  it("stops delivering readings once the caller has unsubscribed", async () => {
    // Given a subscription the caller has already let go of
    const feed = aStreamOf(A_READING, A_LATER_READING);
    const seen: HostStatsEventLike[] = [];
    const subscription = subscribeHostStats(() => feed.stream, (event) => seen.push(event));
    subscription.unsubscribe();

    // When the feed would have gone on emitting
    await settle();

    // Then nothing arrived after the caller let go
    expect(seen).toEqual([]);
  });

  it("cancels the call when the caller unsubscribes, on a stream that offers no return()", async () => {
    // Given a feed shaped as the real Connect client's — `next` only, no `return`
    const feed = aConnectShapedStream(A_READING);
    const subscription = subscribeHostStats(feed.open, () => {});
    await settle();

    // When the caller lets go
    subscription.unsubscribe();
    await settle();

    // Then the call itself was cancelled; there is no iterator to release, so nothing else could
    // have ended it.
    expect(feed.wasCancelled()).toBe(true);
  });

  it("releases a stream that ends of its own accord", async () => {
    // Given a feed that completes after one reading
    const feed = aStreamThatEndsAfter(A_READING);

    // When the caller subscribes and the feed runs out
    const subscription = subscribeHostStats(() => feed.stream, () => {});
    await settle();

    // Then it was released rather than left half-consumed
    expect(feed.wasClosed()).toBe(true);
    subscription.unsubscribe();
  });

  it("survives a transport that cannot open the stream at all", async () => {
    // Given a transport that refuses
    const seen: HostStatsEventLike[] = [];

    // When the caller subscribes
    const subscription = subscribeHostStats(aStreamThatCannotOpen(), (event) => seen.push(event));
    await settle();

    // Then no reading arrived and the refusal did not escape to the caller
    expect(seen).toEqual([]);
    subscription.unsubscribe();
  });

  it("keeps the reading it already delivered when the feed fails", async () => {
    // Given a feed that drops after one reading
    const feed = aStreamThatFailsAfter(A_READING);
    const seen: HostStatsEventLike[] = [];

    // When the caller subscribes
    const subscription = subscribeHostStats(() => feed.stream, (event) => seen.push(event));
    await settle();

    // Then the reading it did deliver stands
    expect(seen).toEqual([A_READING]);
    subscription.unsubscribe();
  });

  it("closes the stream when the feed fails", async () => {
    // Given a feed that drops after one reading
    const feed = aStreamThatFailsAfter(A_READING);

    // When the caller subscribes and the feed fails
    const subscription = subscribeHostStats(() => feed.stream, () => {});
    await settle();

    // Then the stream is closed rather than left dangling
    expect(feed.wasClosed()).toBe(true);
    subscription.unsubscribe();
  });
});
