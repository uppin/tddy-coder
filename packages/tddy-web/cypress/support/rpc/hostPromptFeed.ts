/**
 * Test double for **`ConnectionService.StreamHostPrompts`** — the one channel on which a host asks
 * the browser a question (PRD: `docs/ft/web/1-WIP/PRD-2026-09-06-agent-add-key.md`).
 *
 * The real stream stays open for the life of the screen and is **silent almost all of the time**;
 * it carries a frame only when an operation on that host has blocked on an answer. The fake mirrors
 * that: its generator never completes, and a spec drives it by calling {@link HostPromptFeed.raise}
 * at the moment it wants the question to land — so a test can state "the operator starts an add,
 * *then* the host asks for the passphrase" rather than mounting into an already-decided state.
 *
 * Modelled on `sessionNotificationFeed.ts`, which fakes the other always-open daemon feed.
 */

import { create } from "@bufbuild/protobuf";
import {
  HostPromptEventSchema,
  HostPromptKind,
  type HostPromptEvent,
} from "../../../src/gen/connection_pb";

/** One question a host is waiting on, as a spec states it. */
export interface HostPromptFrame {
  readonly promptId: string;
  readonly daemonInstanceId: string;
  /** What is being unlocked — the key path the operator named. Never a secret. */
  readonly subject: string;
  /** SPKI DER of the host's RSA public key. */
  readonly hostPublicKey: Uint8Array;
  readonly hostPublicKeyFingerprint: string;
  /** Defaults to a minute out, which is what a live daemon stamps. */
  readonly expiresAtUnixMs?: number;
}

export interface HostPromptFeed {
  /** The `StreamHostPrompts` handler, spreadable into a `ConnectionService` backend. */
  readonly handlers: Record<string, unknown>;
  /** Deliver one prompt to every subscribed client, now. */
  readonly raise: (frame: HostPromptFrame) => void;
  /** How many clients have opened the stream — pins "one subscription per host, not per render". */
  readonly subscriptionCount: () => number;
}

export function aHostPromptFeed(): HostPromptFeed {
  const deliverToSubscriber: Array<(event: HostPromptEvent) => void> = [];
  let subscriptions = 0;

  return {
    handlers: {
      async *streamHostPrompts(): AsyncGenerator<HostPromptEvent> {
        subscriptions += 1;
        const queued: HostPromptEvent[] = [];
        let wake: () => void = () => undefined;
        deliverToSubscriber.push((event) => {
          queued.push(event);
          wake();
        });
        // Never returns: the real feed lives as long as the screen does, and is silent for nearly
        // all of it.
        for (;;) {
          while (queued.length > 0) {
            yield queued.shift() as HostPromptEvent;
          }
          await new Promise<void>((resolve) => {
            wake = resolve;
          });
        }
      },
    },

    raise: (frame: HostPromptFrame) => {
      // A prompt raised with nobody listening reaches nobody — exactly as on the wire. Silently
      // dropping it would turn a mis-sequenced spec into a mysteriously failing assertion three
      // lines later, so the helper says what went wrong instead.
      if (deliverToSubscriber.length === 0) {
        throw new Error(
          "raised a host prompt before the screen subscribed — wait for the stream to open first",
        );
      }
      const event = create(HostPromptEventSchema, {
        promptId: frame.promptId,
        daemonInstanceId: frame.daemonInstanceId,
        kind: HostPromptKind.SSH_KEY_PASSPHRASE,
        subject: frame.subject,
        hostPublicKey: frame.hostPublicKey,
        hostPublicKeyFingerprint: frame.hostPublicKeyFingerprint,
        expiresAtUnixMs: BigInt(frame.expiresAtUnixMs ?? Date.now() + 60_000),
      });
      deliverToSubscriber.forEach((deliver) => deliver(event));
    },

    subscriptionCount: () => subscriptions,
  };
}
