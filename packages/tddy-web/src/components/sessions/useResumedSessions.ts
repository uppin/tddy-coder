import type { SessionEntry } from "../../gen/session_pb";

/**
 * Sessions resumed in this browser that the session list does not yet report as live.
 *
 * The pane's base view is derived from a session's liveness, and liveness only refreshes on the
 * drawer's 2s `ListSessions` poll — so after a successful `ResumeSession` the stopped view would
 * linger until the next poll. A successful resume marks the session live here at once; the mark is
 * dropped as soon as a listing reports the session live itself, after which the listing alone
 * decides again. The view stays derived — this is a fresher liveness fact, not navigation.
 *
 * PRD: docs/ft/web/1-WIP/PRD-2026-11-02-dormant-agent-tab.md (`#session-tabs 6/7`)
 */
export type ResumedSessions = ReadonlySet<string>;

/** `resumed` with `sessionId` marked live after a successful resume. */
export function markResumed(resumed: ResumedSessions, sessionId: string): ResumedSessions {
  // TODO(dormant-agent-tab): implement
  void sessionId;
  return resumed;
}

/** `resumed` after a listing: a session the listing reports live needs no mark any more. */
export function afterListing(
  resumed: ResumedSessions,
  listed: readonly Pick<SessionEntry, "sessionId" | "isActive">[],
): ResumedSessions {
  // TODO(dormant-agent-tab): implement
  void listed;
  return resumed;
}

/** Whether `session` is live — by the listing, or by a resume the listing has not caught up with. */
export function isLive(
  resumed: ResumedSessions,
  session: Pick<SessionEntry, "sessionId" | "isActive">,
): boolean {
  // TODO(dormant-agent-tab): implement
  void resumed;
  return session.isActive;
}
