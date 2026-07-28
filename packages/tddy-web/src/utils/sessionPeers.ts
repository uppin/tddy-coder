import type { SessionEntry } from "../gen/connection_pb";

/**
 * Derive the peer agent sessions of a given session.
 *
 * A peer is a session whose `orchestratorSessionId` equals the given session's id — i.e. a child
 * session spawned via the existing `stack_parent` / orchestrator path (the same path
 * `PrStackScreen` uses to spawn PR-stack children). Peers share the orchestrator's workspace but
 * run their own agent backend.
 *
 * The current session itself is never included, even if it malformedly self-references. Order
 * follows the input list (the drawer already sorts by recency); no reordering is applied here so
 * the section's order matches the drawer's.
 *
 * @param sessions  The full session list (e.g. the drawer's `sessions` prop).
 * @param currentSessionId  The session id whose peers to derive.
 * @returns The peers of the current session, in input order. Empty when `currentSessionId` is
 *          falsy or no sessions match.
 */
export function sessionPeers(
  sessions: SessionEntry[],
  currentSessionId: string,
): SessionEntry[] {
  if (!currentSessionId) return [];
  return sessions.filter(
    (session) =>
      session.sessionId !== currentSessionId &&
      session.orchestratorSessionId === currentSessionId,
  );
}
