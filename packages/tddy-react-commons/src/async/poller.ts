// TODO(react-commons): implement — contract surface; bodies land with the implementation.
import { type SchedulerLike } from 'rxjs';
import { type Store } from '../stores/index';
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
    throw new Error("TODO(react-commons): implement");
}
