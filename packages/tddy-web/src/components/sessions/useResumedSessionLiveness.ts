import { useCallback, useEffect, useState } from "react";
import type { SessionEntry } from "../../gen/session_pb";
import { afterListing, markResumed, type ResumedSessions } from "./useResumedSessions";

const NOTHING_RESUMED: ResumedSessions = new Set();

/**
 * The drawer's resumed-session hint: the sessions this browser resumed that the listing has not yet
 * caught up with. `listed` is the drawer's merged session list, so every list snapshot settles the
 * marks it answers for; `markSessionResumed` is called once a `ResumeSession` has landed.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-02-dormant-agent-tab.md (`#session-tabs 6/7`)
 */
export function useResumedSessionLiveness(listed: readonly SessionEntry[]): {
  resumed: ResumedSessions;
  markSessionResumed: (sessionId: string) => void;
} {
  const [resumed, setResumed] = useState<ResumedSessions>(NOTHING_RESUMED);

  useEffect(() => {
    setResumed((current) => afterListing(current, listed));
  }, [listed]);

  const markSessionResumed = useCallback((sessionId: string) => {
    setResumed((current) => markResumed(current, sessionId));
  }, []);

  return { resumed, markSessionResumed };
}
