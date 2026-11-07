import { dequal } from 'dequal';
import { asyncScheduler, timer, type SchedulerLike, type Subscription } from 'rxjs';

import { createStore, type Store } from '../stores/index';

import type { RequestState } from './index';

/**
 * A call repeated on an interval — what a browser has instead of a server stream.
 *
 * Connect bidi needs end-to-end HTTP/2, which the browser transport does not give
 * us, so following long-running work means asking again.
 */
export interface Poller<T> {
  readonly latest: Store<RequestState<T>>;
  /** Whether the loop is running. Separate from `latest`: a stopped poller still
   *  holds the last answer it got, and a view usually wants to keep showing it. */
  readonly running: Store<boolean>;
  start(): void;
  stop(): void;
  /**
   * Poll now instead of waiting out the interval, because something the caller
   * saw makes the last answer stale. A refresh during an in-flight poll queues
   * exactly one follow-up, because the in-flight answer may predate the change.
   */
  refresh(): void;
  teardown(): void;
}

/**
 * Whether anybody can see the page. A hidden tab polls for nobody, so the poller
 * pauses while the source reports it hidden.
 */
export interface VisibilitySource {
  isVisible(): boolean;
  subscribe(onChange: () => void): () => void;
}

/** A runtime with no `document` (Node, a worker) has no tab to hide. */
const hasDocument = (): boolean => typeof document !== 'undefined';

const documentVisibility: VisibilitySource = {
  isVisible: () => !hasDocument() || document.visibilityState !== 'hidden',
  subscribe: onChange => {
    if (!hasDocument()) {
      return () => undefined;
    }
    document.addEventListener('visibilitychange', onChange);
    return () => document.removeEventListener('visibilitychange', onChange);
  },
};

export interface PollOptions<T> {
  readonly intervalMs: number;
  /** Compares successful answers before publishing. Defaults to structural equality.
   * Answers must be immutable; use an explicit comparator for other value semantics. */
  readonly equality?: (previous: T, next: T) => boolean;
  /**
   * The delay after a successful poll, chosen from its answer. Defaults to
   * `intervalMs`. Lets a caller poll fast only while the answer says something
   * is still changing.
   */
  readonly nextIntervalMs?: (value: T) => number;
  /**
   * How long to wait after a failed poll, given how many have failed in a row.
   * Defaults to `intervalMs` — no backoff. Supplied rather than built in because
   * "keep retrying at the same rate" is right for a local dev server and wrong
   * for a rate-limited one, and the poller cannot know which it is talking to.
   */
  readonly backoffMs?: (consecutiveFailures: number) => number;
  /**
   * rxjs scheduler for the interval. Tests pass a virtual one so a poll loop is
   * exercised without real time passing.
   */
  readonly scheduler?: SchedulerLike;
  /** Defaults to the document's visibility. */
  readonly visibility?: VisibilitySource;
}

export function createPoller<T>(call: () => Promise<T>, options: PollOptions<T>): Poller<T> {
  const {
    intervalMs,
    equality = dequal,
    nextIntervalMs = () => intervalMs,
    backoffMs = () => intervalMs,
    scheduler = asyncScheduler,
    visibility = documentVisibility,
  } = options;

  const latest = createStore<RequestState<T>>(() => ({ status: 'idle' }));
  const running = createStore<boolean>(() => false);

  let timerSubscription: Subscription | undefined;
  let unsubscribeVisibility: (() => void) | undefined;
  let loopActive = false;
  let tornDown = false;
  let consecutiveFailures = 0;
  let pollInFlight = false;
  let followUpQueued = false;

  const canPoll = (): boolean => !tornDown && loopActive && visibility.isVisible();

  const clearTimer = (): void => {
    timerSubscription?.unsubscribe();
    timerSubscription = undefined;
  };

  const scheduleNext = (delayMs: number): void => {
    if (!canPoll()) {
      return;
    }
    clearTimer();
    timerSubscription = timer(delayMs, scheduler).subscribe(() => {
      void executePoll();
    });
  };

  const executePoll = async (): Promise<void> => {
    if (!canPoll() || pollInFlight) {
      return;
    }
    clearTimer();
    pollInFlight = true;
    try {
      const value = await call();
      if (!tornDown && loopActive) {
        const delayMs = nextIntervalMs(value);
        consecutiveFailures = 0;
        const previous = latest.getState();
        if (previous.status !== 'ready' || !equality(previous.value, value)) {
          latest.next({ status: 'ready', value });
        }
        scheduleNext(delayMs);
      }
    } catch (error) {
      if (!tornDown && loopActive) {
        consecutiveFailures += 1;
        latest.next({ status: 'error', error });
        scheduleNext(backoffMs(consecutiveFailures));
      }
    } finally {
      pollInFlight = false;
    }
    if (followUpQueued) {
      followUpQueued = false;
      void executePoll();
    }
  };

  const refresh = (): void => {
    if (!canPoll()) {
      return;
    }
    if (pollInFlight) {
      followUpQueued = true;
      return;
    }
    void executePoll();
  };

  const onVisibilityChange = (): void => {
    if (!visibility.isVisible()) {
      clearTimer();
      return;
    }
    // An answer still in flight schedules the next poll itself.
    if (!pollInFlight) {
      void executePoll();
    }
  };

  const start = (): void => {
    if (tornDown || loopActive) {
      return;
    }
    loopActive = true;
    running.setState(true);
    unsubscribeVisibility = visibility.subscribe(onVisibilityChange);
    // A poll from before the last stop may still be in flight. Its answer can belong to the old caller state.
    refresh();
  };

  const stop = (): void => {
    if (!loopActive) {
      return;
    }
    loopActive = false;
    followUpQueued = false;
    running.setState(false);
    clearTimer();
    unsubscribeVisibility?.();
    unsubscribeVisibility = undefined;
  };

  const teardown = (): void => {
    tornDown = true;
    stop();
  };

  return { latest, running, start, stop, refresh, teardown };
}
