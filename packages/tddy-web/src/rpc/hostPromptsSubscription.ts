/**
 * The read loop behind `useHostPrompts`, as a plain function so that closing it is observable.
 *
 * Same split, and for the same reason, as `hostStatsSubscription`: a component test cannot see a
 * subscription being let go, because `createRouterTransport` — the in-memory testkit's transport —
 * propagates neither an abort nor a consumer's `break` to the server handler. A hand-rolled async
 * iterable can, so the loop lives here with unit tests and the hook is a thin wrapper.
 *
 * The leak matters more here than it does for stats. `StreamHostPrompts` is **silent almost all of
 * the time** — that is its normal state, and the daemon-side handler carries a `tokio::select!` on
 * `tx.closed()` precisely because of it. A `for await` parked on a first frame that may never come
 * has no reachable `break`, so every host a screen subscribes to and never hears from would leak a
 * subscription for the life of the page.
 *
 * Unlike the stats loop, a feed failure is handed to the caller rather than written to
 * `console.debug`. A prompt feed that has quietly died looks exactly like one that is merely idle,
 * so "the feed dropped" is a fact a surface may want to show — and it is the only way to state, as a
 * test, that an abort of our own making is *not* reported.
 *
 * PRD: `docs/ft/web/1-WIP/PRD-2026-09-06-agent-add-key.md`
 */

/**
 * The part of a `HostPromptEvent` this loop passes on — structurally satisfied by the generated
 * message, so the hook hands over the wire type unchanged and tests can hand over a literal.
 */
export interface HostPromptEventLike {
  promptId: string;
  daemonInstanceId: string;
  subject: string;
  /** SPKI DER of the host's RSA public key; the answer is encrypted under it. */
  hostPublicKey: Uint8Array;
  hostPublicKeyFingerprint: string;
}

export interface HostPromptsSubscription {
  /** Stop reading and close the stream, whether or not it has emitted. Safe to call twice. */
  unsubscribe: () => void;
}

/**
 * Read `open()`'s stream, handing each prompt to `onPrompt` until the caller unsubscribes.
 *
 * @param open opens the stream; called once, on subscribe. The signal is what actually ends the
 *        call — a Connect client's iterator has `next` and nothing else, so releasing it cancels
 *        nothing.
 * @param onPrompt receives every prompt, in the order the daemon raised them, until unsubscribed.
 * @param onFeedFailure receives a feed that ended while the caller was still subscribed. An abort
 *        this loop performed itself on `unsubscribe` is not a failure and never reaches it.
 */
export function subscribeHostPrompts(
  open: (signal: AbortSignal) => AsyncIterable<HostPromptEventLike>,
  onPrompt: (prompt: HostPromptEventLike) => void,
  onFeedFailure: (error: unknown) => void = () => undefined,
): HostPromptsSubscription {
  // TODO: unimplemented — the loop, its cancellation and its failure reporting are still to be written.
  void open;
  void onPrompt;
  void onFeedFailure;
  throw new Error("subscribeHostPrompts is not implemented");
}
