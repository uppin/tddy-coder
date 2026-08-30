/**
 * The session drawer's indicator dot, and the pieces that decide what it shows.
 *
 * PRD: docs/ft/daemon/session-notifications.md (FR4–FR6).
 *
 * The drawer paints this dot in two places — a row of the expanded list, and a row of the collapsed
 * 12px strip — at different sizes but with identical meaning. They live here as **one** component
 * rather than two JSX blocks sharing a colour map, because a shared map still let the two drift on
 * everything around it: before this changeset the strip was missing a state the expanded list had,
 * and after it the strip was still writing its own `status === "working" && …` by hand.
 */

import React, { useSyncExternalStore } from "react";
import type { SessionEntry } from "../../gen/connection_pb";
import { sessionIndicatorFor, type SessionIndicator } from "../../lib/sessionIndicator";
import { sessionNotificationRegistry } from "./sessionNotificationRegistry";
import { cn } from "../../lib/utils";

/**
 * The colour each indicator paints its dot.
 *
 * `working` is the same green as `connected` on purpose: it is the *same* claim (the session is
 * alive and healthy), and what distinguishes it is the movement, not the hue. A fourth colour would
 * make "busy" read as a different kind of condition than "idle".
 *
 * Typed by `SessionIndicator` rather than `string`, so the map is total: a fifth indicator is a
 * compile error here instead of a dot that silently falls back to grey.
 */
export const SESSION_INDICATOR_COLOR: Record<SessionIndicator, string> = {
  connected: "bg-green-500",
  working: "bg-green-500",
  disconnected: "bg-gray-400",
  "needs-input": "bg-yellow-500",
};

/** The class carrying the fade-in/fade-out animation; see `sessionIndicatorDotStyles.ts`. */
export const SESSION_INDICATOR_BLINK_CLASS = "tddy-session-dot--working";

/**
 * The indicator one drawer row should show right now, subscribing the caller to that session's
 * notifications.
 *
 * The subscription is per row rather than per drawer because `sessionNotificationRegistry` replaces
 * only the state of the session a notification names — so a frame for one session re-renders that
 * row alone, and a busy daemon does not repaint twelve rows per turn.
 *
 * `Date.now()` is read on each render rather than held on a timer: the only time-dependent rule is
 * the blink window's decay, and the drawer already re-renders on the session-list poll, so a settled
 * blink is picked up within one poll of the window elapsing. A dedicated interval would buy a couple
 * of seconds' precision on a 30-second window at the cost of a timer per row.
 */
export function useSessionIndicator(session: SessionEntry): SessionIndicator {
  const state = useSyncExternalStore(
    (listener) => sessionNotificationRegistry.subscribe(listener),
    () => sessionNotificationRegistry.get(session.sessionId),
  );
  return sessionIndicatorFor(session, state, Date.now());
}

interface SessionIndicatorDotProps {
  session: SessionEntry;
  /** Tailwind size classes. The collapsed strip renders a slightly larger dot, being all it shows. */
  sizeClassName?: string;
}

/**
 * The dot itself. `data-status` carries the indicator token so a row states its meaning
 * (`working`, `needs-input`, …) rather than only its colour, and the test id is the same in both
 * placements — the strip and the expanded list are mutually exclusive, so one id addresses
 * whichever the drawer is currently showing.
 */
export function SessionIndicatorDot({
  session,
  sizeClassName = "h-2 w-2",
}: SessionIndicatorDotProps) {
  const status = useSessionIndicator(session);

  return (
    <span
      data-testid={`sessions-drawer-item-status-${session.sessionId}`}
      data-status={status}
      className={cn(
        "flex-shrink-0 rounded-full",
        sizeClassName,
        SESSION_INDICATOR_COLOR[status],
        status === "working" && SESSION_INDICATOR_BLINK_CLASS,
      )}
    />
  );
}
