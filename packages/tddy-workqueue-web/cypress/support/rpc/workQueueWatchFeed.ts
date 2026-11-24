/**
 * A scripted `WatchWorkQueue`: each subscription the view opens is recorded, and the test decides what
 * it delivers and when — a snapshot, a failure that ends it. Mirrors the daemon's watch, which sends
 * the queue as it stands, then nothing until something changes.
 */

import { create } from "@bufbuild/protobuf";
import { Code, ConnectError } from "@connectrpc/connect";
import {
  WorkQueueSnapshotSchema,
  type WatchWorkQueueRequest,
  type WorkQueueSnapshot,
} from "tddy-client-web/gen/work_queue_pb";

type Delivery = { kind: "snapshot"; snapshot: WorkQueueSnapshot } | { kind: "fail"; error: ConnectError };

/** One subscription the view opened. */
export interface WatchSubscription {
  readonly request: WatchWorkQueueRequest;
  /** Deliver the queue `workQueueJson` at `revision`. */
  send(revision: number, workQueueJson: string): void;
  /** End the subscription with `code`. */
  fail(code: Code, message: string): void;
  /** How many snapshots the view has taken from this subscription and asked past. */
  readonly consumed: number;
}

export interface WorkQueueWatchFeed {
  /** Every subscription opened so far, oldest first. */
  readonly subscriptions: WatchSubscription[];
  /** Run for each subscription as it opens — how a test scripts what every (re)subscription gets. */
  onSubscribe(script: (subscription: WatchSubscription, index: number) => void): void;
  /** The `watchWorkQueue` implementation to register on a backend. */
  readonly impl: (request: WatchWorkQueueRequest) => AsyncIterable<WorkQueueSnapshot>;
}

export function aWorkQueueWatchFeed(): WorkQueueWatchFeed {
  const subscriptions: WatchSubscription[] = [];
  let script: (subscription: WatchSubscription, index: number) => void = () => undefined;

  async function* watch(request: WatchWorkQueueRequest): AsyncIterable<WorkQueueSnapshot> {
    const pending: Delivery[] = [];
    let wake: (() => void) | null = null;
    const deliver = (delivery: Delivery) => {
      pending.push(delivery);
      wake?.();
      wake = null;
    };
    let consumed = 0;
    const subscription: WatchSubscription = {
      request,
      get consumed() {
        return consumed;
      },
      send: (revision, workQueueJson) =>
        deliver({
          kind: "snapshot",
          snapshot: create(WorkQueueSnapshotSchema, { revision: BigInt(revision), workQueueJson }),
        }),
      fail: (code, message) => deliver({ kind: "fail", error: new ConnectError(message, code) }),
    };
    subscriptions.push(subscription);
    script(subscription, subscriptions.length - 1);
    for (;;) {
      const next = pending.shift();
      if (!next) {
        await new Promise<void>((resolve) => {
          wake = resolve;
        });
        continue;
      }
      if (next.kind === "fail") throw next.error;
      yield next.snapshot;
      consumed += 1;
    }
  }

  return {
    subscriptions,
    onSubscribe: (next) => {
      script = next;
    },
    impl: watch,
  };
}

export { Code };
