/**
 * Test double for **`ConnectionService.StreamSessionNotifications`** — the one daemon-level feed
 * the session drawer subscribes to for every row (PRD:
 * `docs/ft/daemon/1-WIP/PRD-2026-08-29-session-notifications-as-indicators.md`).
 *
 * The real stream stays open for the life of the screen and pushes an event whenever any session
 * on the daemon does something. The fake mirrors that: its generator never completes, and a spec
 * drives it by calling {@link SessionNotificationFeed.push} at the moment it wants the event to
 * land — so a test can state "the row is steady green, *then* activity arrives, *then* it blinks"
 * rather than mounting into an already-decided state.
 */

import { create } from "@bufbuild/protobuf";
import {
  SessionNotificationEventSchema,
  SessionNotificationKind,
  SessionNotificationSource,
  type SessionNotificationEvent,
} from "../../../src/gen/connection_pb";

/** One notification, as a spec states it. */
export interface SessionNotificationFrame {
  readonly sessionId: string;
  /** The session's drawer label, as the daemon resolved it. */
  readonly label: string;
  readonly kind: SessionNotificationKind;
  /** Defaults to `ACTIVITY_STATUS` — the claude-cli hook path most specs model. */
  readonly source?: SessionNotificationSource;
  /** Operator-facing line; the drawer shows it as the row's tooltip. */
  readonly text: string;
  /** Defaults to `Date.now()`, which is what a live daemon stamps. */
  readonly atUnixMs?: number;
}

export interface SessionNotificationFeed {
  /** The `StreamSessionNotifications` handler, spreadable into a `ConnectionService` backend. */
  readonly handlers: Record<string, unknown>;
  /** Deliver one notification to every subscribed client, now. */
  readonly push: (frame: SessionNotificationFrame) => void;
  /** How many clients have opened the stream — pins "one subscription for the whole drawer". */
  readonly subscriptionCount: () => number;
}

/** A session working: the agent reported a tool call or a running turn. */
export function anActivityNotification(
  sessionId: string,
  label: string,
  atUnixMs?: number,
): SessionNotificationFrame {
  return {
    sessionId,
    label,
    kind: SessionNotificationKind.ACTIVITY,
    text: `Session ${label}: agent is working`,
    atUnixMs,
  };
}

/** A session waiting on the operator — the same event that pings Telegram. */
export function anAttentionNotification(
  sessionId: string,
  label: string,
  atUnixMs?: number,
): SessionNotificationFrame {
  return {
    sessionId,
    label,
    kind: SessionNotificationKind.ATTENTION_REQUIRED,
    text: `🔔 Session ${label}: Claude Code needs your input (permission, question, or your next prompt).`,
    atUnixMs,
  };
}

export function aSessionNotificationFeed(): SessionNotificationFeed {
  const deliverToSubscriber: Array<(event: SessionNotificationEvent) => void> = [];
  let subscriptions = 0;

  return {
    handlers: {
      async *streamSessionNotifications(): AsyncGenerator<SessionNotificationEvent> {
        subscriptions += 1;
        const queued: SessionNotificationEvent[] = [];
        let wake: () => void = () => undefined;
        deliverToSubscriber.push((event) => {
          queued.push(event);
          wake();
        });
        // Never returns: the real feed lives as long as the screen does.
        for (;;) {
          while (queued.length > 0) {
            yield queued.shift() as SessionNotificationEvent;
          }
          await new Promise<void>((resolve) => {
            wake = resolve;
          });
        }
      },
    },

    push: (frame: SessionNotificationFrame) => {
      // A push with nobody listening reaches nobody — exactly as on the wire. Silently dropping it
      // would turn a mis-sequenced spec into a mysteriously failing assertion three lines later,
      // so the helper says what went wrong instead.
      if (deliverToSubscriber.length === 0) {
        throw new Error(
          "pushed a session notification before the screen subscribed — wait for the stream to open first",
        );
      }
      const event = create(SessionNotificationEventSchema, {
        sessionId: frame.sessionId,
        label: frame.label,
        kind: frame.kind,
        source: frame.source ?? SessionNotificationSource.ACTIVITY_STATUS,
        text: frame.text,
        atUnixMs: BigInt(frame.atUnixMs ?? Date.now()),
      });
      deliverToSubscriber.forEach((deliver) => deliver(event));
    },

    subscriptionCount: () => subscriptions,
  };
}
