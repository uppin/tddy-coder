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

/** A stream that hands over `events`, then stays open the way the daemon's feed does. */
function aStreamOf(...events: HostStatsEventLike[]) {
  let closed = false;
  let delivered = 0;
  const stream = {
    async *[Symbol.asyncIterator]() {
      try {
        for (const event of events) {
          delivered += 1;
          yield event;
        }
        await new Promise<never>(() => undefined);
      } finally {
        closed = true;
      }
    },
  };
  return {
    stream,
    wasClosed: () => closed,
    deliveredCount: () => delivered,
  };
}

/** A stream that opens and never emits — a host subscribed but not yet reporting. */
function aSilentStream() {
  return aStreamOf();
}

/** A stream that fails after handing over `events`. */
function aStreamThatFailsAfter(...events: HostStatsEventLike[]) {
  let closed = false;
  const stream = {
    async *[Symbol.asyncIterator]() {
      try {
        for (const event of events) yield event;
        throw new Error("daemon dropped the feed");
      } finally {
        closed = true;
      }
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

  it("swallows a failed feed rather than throwing at the caller", async () => {
    // Given a feed that drops after one reading
    const feed = aStreamThatFailsAfter(A_READING);
    const seen: HostStatsEventLike[] = [];

    // When the caller subscribes
    const subscription = subscribeHostStats(() => feed.stream, (event) => seen.push(event));
    await settle();

    // Then the reading it did deliver stands, and the failure did not escape
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
