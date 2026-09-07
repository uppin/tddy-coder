/**
 * Unit tests for the host-prompt subscription — the loop behind `useHostPrompts`, extracted so that
 * closing it is observable.
 *
 * Teardown is the reason this is a plain function rather than a hook, and the reason these tests
 * exist at all. A component test cannot see it: `createRouterTransport` (the in-memory testkit's
 * transport) propagates neither an abort nor a consumer's `break` to the server handler, so no fake
 * backend can count a subscription closing. A hand-rolled async iterable can — the abort signal
 * fires exactly when the consumer lets go.
 *
 * `StreamHostPrompts` is silent almost all of the time; that is its normal state, not a fault. A
 * screenful of hosts that never raise a prompt is therefore the *common* case, and it is the one a
 * `for await` cannot release. The daemon-side handler carries a `tokio::select!` on `tx.closed()`
 * for the same reason; this is that bug's mirror image on the browser side.
 *
 * PRD: `docs/ft/web/1-WIP/PRD-2026-09-06-agent-add-key.md`
 */

import { describe, it, expect } from "bun:test";
import { subscribeHostPrompts, type HostPromptEventLike } from "./hostPromptsSubscription";

const A_PASSPHRASE_PROMPT: HostPromptEventLike = {
  promptId: "prompt-1",
  daemonInstanceId: "workstation-1",
  subject: "/home/ada/.ssh/id_ed25519",
  hostPublicKey: new Uint8Array([0x30, 0x82, 0x01, 0x22]),
  hostPublicKeyFingerprint: "SHA256:ZLBiCcwTvIcQUyRnvSHhpsdgWLLLZtWbBAPtgWNBpAg",
};

const A_SECOND_PROMPT: HostPromptEventLike = {
  ...A_PASSPHRASE_PROMPT,
  promptId: "prompt-2",
  subject: "/home/ada/.ssh/id_rsa",
};

/**
 * A feed shaped exactly as a Connect client's: an iterator with **`next` and nothing else**.
 *
 * `@connectrpc/connect` wraps every server-stream in `{ [Symbol.asyncIterator]: () => ({ next }) }`
 * — its own comment reads "Create a new iterable to omit throw/return" — so in production there is
 * no `return()` to call and releasing the iterator cannot end anything. Only the call's abort signal
 * can. A fake that offered `return()` would let a subscription that never cancels look correct.
 *
 * After `prompts` run out it stays open, which is where this feed spends nearly all of its life.
 */
function aPromptFeedOf(...prompts: HostPromptEventLike[]) {
  let cancelled = false;
  let handedOver = 0;
  const open = (signal: AbortSignal): AsyncIterable<HostPromptEventLike> => ({
    [Symbol.asyncIterator]: () => ({
      next: () => {
        if (handedOver < prompts.length) {
          return Promise.resolve({ value: prompts[handedOver++], done: false as const });
        }
        // Open, with nothing to ask — settled only by the caller giving up.
        return new Promise<IteratorResult<HostPromptEventLike>>((_resolve, reject) => {
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

/** A host that subscribed and has never asked anything — this feed's normal state. */
function aSilentPromptFeed() {
  return aPromptFeedOf();
}

/** A feed the daemon drops after `prompts`, while the caller is still reading it. */
function aPromptFeedThatDropsAfter(...prompts: HostPromptEventLike[]) {
  let handedOver = 0;
  return (): AsyncIterable<HostPromptEventLike> => ({
    [Symbol.asyncIterator]: () => ({
      next: () =>
        handedOver < prompts.length
          ? Promise.resolve({ value: prompts[handedOver++], done: false as const })
          : Promise.reject(new Error("daemon dropped the prompt feed")),
    }),
  });
}

/** Let the subscription's own microtasks run. */
function settle() {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

describe("host prompts subscription", () => {
  it("hands every prompt the host raises to the caller in order", async () => {
    // Given a feed carrying two questions
    const feed = aPromptFeedOf(A_PASSPHRASE_PROMPT, A_SECOND_PROMPT);
    const seen: HostPromptEventLike[] = [];

    // When the caller subscribes
    const subscription = subscribeHostPrompts(feed.open, (prompt) => seen.push(prompt));
    await settle();

    // Then it saw both, newest last
    expect(seen).toEqual([A_PASSPHRASE_PROMPT, A_SECOND_PROMPT]);
    subscription.unsubscribe();
  });

  it("cancels the call when the caller unsubscribes", async () => {
    // Given a live subscription that has already surfaced a prompt
    const feed = aPromptFeedOf(A_PASSPHRASE_PROMPT);
    const subscription = subscribeHostPrompts(feed.open, () => {});
    await settle();

    // When the caller lets go
    subscription.unsubscribe();
    await settle();

    // Then the call itself was cancelled; a Connect stream offers no `return()`, so nothing else
    // could have ended it.
    expect(feed.wasCancelled()).toBe(true);
  });

  it("cancels a feed that has never raised a prompt", async () => {
    // Given the state this feed is in nearly all of the time — subscribed, silent
    const feed = aSilentPromptFeed();
    const subscription = subscribeHostPrompts(feed.open, () => {});
    await settle();

    // When the caller lets go before anything was ever asked
    subscription.unsubscribe();
    await settle();

    // Then that call is cancelled too. Waiting on a first frame that may never come is exactly how
    // a screenful of quiet hosts would leak a subscription each.
    expect(feed.wasCancelled()).toBe(true);
  });

  it("stops delivering prompts once the caller has unsubscribed", async () => {
    // Given a subscription the caller has already let go of
    const feed = aPromptFeedOf(A_PASSPHRASE_PROMPT, A_SECOND_PROMPT);
    const seen: HostPromptEventLike[] = [];
    const subscription = subscribeHostPrompts(feed.open, (prompt) => seen.push(prompt));
    subscription.unsubscribe();

    // When the feed would have gone on asking
    await settle();

    // Then nothing arrived after the caller let go — a dialog raised for a screen that is gone is
    // a secret asked for by nobody.
    expect(seen).toEqual([]);
  });

  it("reports nothing when the call it cancelled itself rejects with an AbortError", async () => {
    // Given a live subscription on a silent feed
    const feed = aSilentPromptFeed();
    const failures: unknown[] = [];
    const subscription = subscribeHostPrompts(feed.open, () => {}, (error) => failures.push(error));
    await settle();

    // When the caller lets go, and the parked call rejects the way an aborted Connect call does
    subscription.unsubscribe();
    await settle();

    // Then that rejection is this loop's own doing and is swallowed, not surfaced as a dead feed
    expect(failures).toEqual([]);
  });

  it("reports a feed the daemon drops while the caller is still subscribed", async () => {
    // Given a feed that dies after one prompt, with the caller still reading
    const failures: unknown[] = [];
    const subscription = subscribeHostPrompts(
      aPromptFeedThatDropsAfter(A_PASSPHRASE_PROMPT),
      () => {},
      (error) => failures.push(error),
    );

    // When the feed drops
    await settle();

    // Then the caller is told exactly once. Without this the swallow above would be vacuous: a
    // subscription that reported nothing at all would satisfy it, and an idle prompt feed is
    // indistinguishable from a dead one.
    expect(failures).toEqual([new Error("daemon dropped the prompt feed")]);
    subscription.unsubscribe();
  });
});
