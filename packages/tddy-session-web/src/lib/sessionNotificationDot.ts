/**
 * Which drawer dot one session notification lights, if any.
 *
 * Attention lights yellow and activity blinks green — except a session's end (source
 * `SESSION_ENDED`, `#activity-wake` 1/3), which is attention for an orchestrator's inbox but lights
 * nothing here: an ended session already reads as stopped. A kind this build does not know lights
 * nothing either, so a newer daemon's event leaves the row as it was.
 *
 * PRD: docs/ft/daemon/1-WIP/PRD-2026-11-16-session-ended-notice.md.
 */

import {
  SessionNotificationKind,
  SessionNotificationSource,
  type SessionNotificationEvent,
} from "tddy-client-web/gen/activity_pb";

export type NotificationDot = "attention" | "activity" | null;

export function dotForNotification(
  event: Pick<SessionNotificationEvent, "kind" | "source">,
): NotificationDot {
  if (event.source === SessionNotificationSource.SESSION_ENDED) {
    return null;
  }
  switch (event.kind) {
    case SessionNotificationKind.ATTENTION_REQUIRED:
      return "attention";
    case SessionNotificationKind.ACTIVITY:
      return "activity";
    default:
      return null;
  }
}
