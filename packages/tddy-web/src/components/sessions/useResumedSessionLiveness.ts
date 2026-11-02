import { useCallback, useEffect, useMemo, useState } from "react";
import type { SessionEntry } from "../../gen/session_pb";
import {
  afterListing,
  markResumed,
  withResumedLiveness,
  type ResumedSessions,
} from "./resumedSessions";

const NOTHING_RESUMED: ResumedSessions = new Set();

/**
 * The drawer's resumed-session hint: the sessions this browser resumed that the listing has not yet
 * caught up with. `listed` is the drawer's merged session list, so every list snapshot settles the
 * marks it answers for; `markSessionResumed` is called once a `ResumeSession` has landed.
 *
 * `liveSelectedSession` is `selectedSession` as the pane should read it: live from the moment its
 * resume landed, so the Agent tab goes live at once rather than at the next poll. Only the pane reads
 * it; the drawer's activation and liveness effects keep working from the listing.
 *
 * PRD: docs/ft/web/inactive-session-activities.md (`#session-tabs 6/7`)
 */
export function useResumedSessionLiveness(
  listed: readonly SessionEntry[],
  selectedSession: SessionEntry | null,
): {
  liveSelectedSession: SessionEntry | null;
  markSessionResumed: (sessionId: string) => void;
} {
  const [resumed, setResumed] = useState<ResumedSessions>(NOTHING_RESUMED);

  useEffect(() => {
    setResumed((current) => afterListing(current, listed));
  }, [listed]);

  const markSessionResumed = useCallback((sessionId: string) => {
    setResumed((current) => markResumed(current, sessionId));
  }, []);

  const liveSelectedSession = useMemo(
    () => (selectedSession ? withResumedLiveness(resumed, selectedSession) : null),
    [resumed, selectedSession],
  );

  return { liveSelectedSession, markSessionResumed };
}
