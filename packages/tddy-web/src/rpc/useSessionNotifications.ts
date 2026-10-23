/**
 * The session drawer's one notification feed.
 *
 * Subscribes to `ActivityService.StreamSessionNotifications` for the selected daemon over the
 * shared common-room LiveKit connection (`useDaemonClient`, like `useHostStats`) and folds every
 * event into `sessionNotificationRegistry`, which the drawer's rows read through
 * `useSyncExternalStore`.
 *
 * The stream is **daemon-level, not session-level**: it carries every session on the daemon, so a
 * drawer of twelve rows opens one subscription rather than twelve (NFR1). That is also why the dots'
 * state is not returned — it is the registry write, and a row that wants it reads it from there
 * rather than through a prop drilled down the drawer. What is returned is whether the feed is live:
 * `error` says why it stopped, so the drawer can tell the operator its dots are no longer updating.
 * It clears when a frame arrives on a new subscription, and when the client changes or goes away —
 * a failure is about the subscription it happened on.
 *
 * PRD: docs/ft/daemon/session-notifications.md (FR3, NFR1).
 */

import { useEffect, useState } from "react";
import { ActivityService, SessionNotificationKind } from "../gen/activity_pb";
import { useDaemonClient } from "./selectedDaemon";
import { useAuthContext } from "../hooks/authProvider";
import { sessionNotificationRegistry } from "../components/sessions/sessionNotificationRegistry";
import { rpcFailureText } from "./rpcFailure";

export interface SessionNotificationsFeed {
  /** Why the feed stopped, ready to render; `null` while it is live (or not yet open). */
  readonly error: string | null;
}

export function useSessionNotifications(): SessionNotificationsFeed {
  const client = useDaemonClient(ActivityService);
  const { sessionToken } = useAuthContext();
  const token = sessionToken ?? "";
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setError(null);
    // No client means no daemon is reachable: the drawer's own reads already say "not connected"
    // for the selected host, so the feed adds no second copy of that message.
    if (!client) return;
    let cancelled = false;

    (async () => {
      try {
        for await (const event of client.streamSessionNotifications({ sessionToken: token })) {
          if (cancelled) break;
          setError(null);
          // The daemon stamps the moment the notification happened; the registry keeps the newest
          // per session, so a replayed or out-of-order frame cannot walk a dot backwards.
          const atMs = Number(event.atUnixMs);
          if (event.kind === SessionNotificationKind.ATTENTION_REQUIRED) {
            sessionNotificationRegistry.recordAttention(event.sessionId, atMs);
          } else if (event.kind === SessionNotificationKind.ACTIVITY) {
            sessionNotificationRegistry.recordActivity(event.sessionId, atMs);
          }
          // Any other kind is a newer daemon telling us about something this build has no dot for.
          // Dropping it leaves the row reading exactly as it did before — the honest rendering of a
          // notification we cannot interpret.
        }
      } catch (err) {
        // A stream aborted on unmount surfaces as an AbortError; ignore it. Any other error while
        // still mounted leaves the already-recorded notifications in place — the dots stop updating
        // rather than resetting to a fabricated "nothing outstanding" — and is reported.
        if (!cancelled) setError(rpcFailureText(err));
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [client, token]);

  return { error };
}
