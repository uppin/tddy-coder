import type { Client } from "@connectrpc/connect";
import type { ActivityService } from "../../gen/activity_pb";
import type { SessionEntry } from "../../gen/session_pb";
import { SessionActivitiesPane } from "./SessionActivitiesPane";
import { rosterHalfOf } from "./sessionRosterHalf";

/**
 * The inspector's Activity tab for a stopped session: its recorded transcript, read through the same
 * pane the stopped session's main view used before its Agent tab showed the greyed terminal output.
 * Only offered for a stopped session (`DORMANT_INSPECTOR_TAB_NAMES`); a live session's transcript is
 * the Agent Activity sidebar's.
 *
 * PRD: docs/ft/web/inactive-session-activities.md (`#session-tabs 6/7`)
 */
export interface SessionInspectorActivityTabProps {
  session: SessionEntry;
  sessionToken: string;
  /** The activity service the drawer was given, so the transcript reads from the session's own host. */
  client?: Client<typeof ActivityService>;
}

export function SessionInspectorActivityTab({
  session,
  sessionToken,
  client,
}: SessionInspectorActivityTabProps) {
  return (
    <SessionActivitiesPane
      sessionId={session.sessionId}
      sessionToken={sessionToken}
      rosterHalf={rosterHalfOf(session)}
      client={client}
    />
  );
}
