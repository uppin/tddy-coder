/**
 * The session drawer's one notification feed.
 *
 * Subscribes to `ConnectionService.StreamSessionNotifications` for the selected daemon over the
 * shared common-room LiveKit connection (`useDaemonClient`, like `useHostStats`) and folds every
 * event into `sessionNotificationRegistry`, which the drawer's rows read through
 * `useSyncExternalStore`.
 *
 * The stream is **daemon-level, not session-level**: it carries every session on the daemon, so a
 * drawer of twelve rows opens one subscription rather than twelve (NFR1). That is also why this hook
 * returns nothing — its whole output is the registry write, and a row that wants the state reads it
 * from there rather than through a prop drilled down the drawer.
 *
 * PRD: docs/ft/daemon/session-notifications.md (FR3, NFR1).
 */

import { useEffect } from "react";
import { ConnectionService, SessionNotificationKind } from "../gen/connection_pb";
import { useDaemonClient } from "./selectedDaemon";
import { useAuthContext } from "../hooks/authProvider";
import { sessionNotificationRegistry } from "../components/sessions/sessionNotificationRegistry";

export function useSessionNotifications(): void {
  const client = useDaemonClient(ConnectionService);
  const { sessionToken } = useAuthContext();
  const token = sessionToken ?? "";

  useEffect(() => {
    if (!client) return;
    let cancelled = false;

    (async () => {
      try {
        for await (const event of client.streamSessionNotifications({ sessionToken: token })) {
          if (cancelled) break;
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
        // still mounted leaves the already-recorded notifications in place: the dots stop updating
        // rather than resetting to a fabricated "nothing outstanding".
        if (!cancelled) {
          console.debug("[useSessionNotifications] streamSessionNotifications error", err);
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [client, token]);
}
