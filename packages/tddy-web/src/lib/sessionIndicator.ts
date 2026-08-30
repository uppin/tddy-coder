import type { SessionEntry } from "../gen/connection_pb";

/**
 * The four states a session's drawer dot can be in.
 *
 * A superset of {@link ConnectionStatus} (`utils/connectionStatusForSession.ts`), which this
 * module replaces: the three poll-derived tokens plus `"working"`, which only the notification
 * stream can tell us about.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-08-29-session-notifications-as-indicators.md § Indicator model.
 */
export type SessionIndicator = "disconnected" | "connected" | "working" | "needs-input";

/**
 * How long a session keeps blinking after its last reported activity.
 *
 * The window exists so the dot settles on its own: an agent that dies mid-turn sends no closing
 * notification, and without a decay its row would claim to be working for as long as the dashboard
 * stayed open.
 */
export const ACTIVITY_BLINK_WINDOW_MS = 30_000;

/**
 * The three moments the indicator is derived from, as
 * `components/sessions/sessionNotificationRegistry.ts` records them. `0` means "never" for all
 * three — no notification has landed, or the operator has not opened the session yet.
 */
export interface SessionNotificationState {
  /** When the session last reported an `ACTIVITY` notification. */
  readonly lastActivityAtMs: number;
  /** When the session last raised an `ATTENTION_REQUIRED` notification. */
  readonly attentionAtMs: number;
  /** When the operator last selected the session in the drawer. */
  readonly seenAtMs: number;
}

/** The state of a session the notification stream has never mentioned. */
const NOTHING_RECORDED: SessionNotificationState = {
  lastActivityAtMs: 0,
  attentionAtMs: 0,
  seenAtMs: 0,
};

/**
 * Picks the dot a drawer row shows, from what the session reports and what it has notified.
 *
 * The order of the rules is the whole decision, and each step exists for a reason:
 *
 * 1. **Liveness first.** A dead session's dot is grey however recently it worked or asked for
 *    input — answering a question nobody is listening to reaches nothing. This preserves the
 *    reasoning (and the behaviour) of `connectionStatusForSession`, which this supersedes.
 * 2. **A pending elicitation is not dismissible by looking at it.** It is persisted, still-
 *    unanswered state; clearing it on a glance would make the drawer claim the operator had dealt
 *    with something they had not. It clears when the elicitation is actually answered.
 * 3. **Attention outstanding.** Notification-driven yellow *is* the dismissible half: it does not
 *    age out, and only viewing the session clears it.
 * 4. **Activity newer than the last view, inside the blink window.** Both conditions matter —
 *    viewing settles the row, and the window settles it again on its own if nothing more arrives.
 * 5. Otherwise the session is alive with nothing outstanding.
 *
 * `nowMs` is a parameter rather than a `Date.now()` call so the caller owns the tick that re-renders
 * the row, and so the decay is testable without waiting 30 seconds for it.
 */
export function sessionIndicatorFor(
  session: Pick<SessionEntry, "isActive" | "pendingElicitation">,
  state: SessionNotificationState | undefined,
  nowMs: number,
): SessionIndicator {
  if (!session.isActive) {
    return "disconnected";
  }
  if (session.pendingElicitation) {
    return "needs-input";
  }

  const { lastActivityAtMs, attentionAtMs, seenAtMs } = state ?? NOTHING_RECORDED;
  if (attentionAtMs > seenAtMs) {
    return "needs-input";
  }
  // Inclusive at the boundary: an activity exactly one window old is still the frame that arrived
  // within it, and excluding it would drop the last blink early.
  if (lastActivityAtMs > seenAtMs && nowMs - lastActivityAtMs <= ACTIVITY_BLINK_WINDOW_MS) {
    return "working";
  }
  return "connected";
}
