import type { SessionEntry } from "../gen/connection_pb";
import { connectionStatusForSession } from "./connectionStatusForSession";

/** One orchestrator session and its stack children, sorted by creation date. */
export interface SessionStackGroup {
  /** The orchestrator session (the one that spawned the children). */
  parent: SessionEntry;
  /** Child sessions in `sortSessionsByCreation` order (oldest-first within the group). */
  children: SessionEntry[];
}

/** Result of {@link groupSessionsByStack}. */
export interface SessionStackGroupResult {
  /** Orchestrator sessions that have at least one child present in the list. */
  groups: SessionStackGroup[];
  /**
   * Sessions that are neither a parent in any group nor a child with a present parent.
   * Includes: plain (non-stack) sessions, and children whose orchestrator is not in the list.
   */
  flat: SessionEntry[];
}

function getOrchestratorSessionId(session: SessionEntry): string {
  return session.orchestratorSessionId;
}

/**
 * Groups sessions by PR-stack relationship.
 *
 * - Sessions whose `orchestratorSessionId` points to another session in the list are *children*;
 *   the referenced session becomes the group *parent*.
 * - Children with a missing parent fall into `flat` (same behaviour as orphan sessions elsewhere).
 * - Non-stack sessions (empty `orchestratorSessionId`) go into `flat`.
 * - Groups are sorted by the parent's `createdAt` (newest-first, matching the drawer sort).
 * - Children within a group are sorted by `createdAt` ascending (oldest child first).
 */
export function groupSessionsByStack(
  sessions: SessionEntry[],
): SessionStackGroupResult {
  if (sessions.length === 0) return { groups: [], flat: [] };

  // Build a map of sessionId → session for fast parent lookup.
  const byId = new Map<string, SessionEntry>();
  for (const s of sessions) {
    byId.set(s.sessionId, s);
  }

  // Separate children (have a present orchestratorSessionId) from potential parents.
  const childrenByParentId = new Map<string, SessionEntry[]>();
  const childSessionIds = new Set<string>();

  for (const s of sessions) {
    const orchId = getOrchestratorSessionId(s);
    if (orchId && byId.has(orchId)) {
      // Valid child: its orchestrator is present in the list.
      childSessionIds.add(s.sessionId);
      const existing = childrenByParentId.get(orchId) ?? [];
      existing.push(s);
      childrenByParentId.set(orchId, existing);
    }
  }

  const groups: SessionStackGroup[] = [];
  const parentSessionIds = new Set<string>(childrenByParentId.keys());

  for (const [parentId, children] of childrenByParentId) {
    const parent = byId.get(parentId)!;
    const sortedChildren = [...children].sort((a, b) =>
      a.createdAt.localeCompare(b.createdAt),
    );
    groups.push({ parent, children: sortedChildren });
  }

  // Sort groups by parent's createdAt (newest-first, matching drawer sort convention).
  groups.sort((a, b) => b.parent.createdAt.localeCompare(a.parent.createdAt));

  // Flat: not a parent and not a child with a present parent.
  const flat = sessions.filter(
    (s) => !parentSessionIds.has(s.sessionId) && !childSessionIds.has(s.sessionId),
  );

  return { groups, flat };
}

/** An active / remaining split of a session list, each side independently stack-grouped. */
export interface SessionActivityPartitions {
  /** Active partition (green/yellow dots), stack-grouped. */
  active: SessionStackGroupResult;
  /** Remaining partition (grey/disconnected dots), stack-grouped. */
  remaining: SessionStackGroupResult;
  /** Raw count of sessions routed to the active partition. */
  activeCount: number;
  /** Raw count of sessions routed to the remaining partition. */
  remainingCount: number;
}

/**
 * Splits sessions into an *active* partition (dot is green or yellow, i.e.
 * {@link connectionStatusForSession} is not `"disconnected"`) and a *remaining* partition
 * (grey/disconnected), then stack-groups each side independently.
 *
 * Because the raw list is filtered *before* grouping, a stack whose sessions differ in activity
 * is split by each session's own dot: a session lands in the partition its status dictates, and
 * stack nesting only survives where parent and child share a partition.
 */
export function partitionSessionsByActivity(
  sessions: SessionEntry[],
): SessionActivityPartitions {
  const activeSessions: SessionEntry[] = [];
  const remainingSessions: SessionEntry[] = [];

  for (const s of sessions) {
    if (connectionStatusForSession(s) !== "disconnected") {
      activeSessions.push(s);
    } else {
      remainingSessions.push(s);
    }
  }

  return {
    active: groupSessionsByStack(activeSessions),
    remaining: groupSessionsByStack(remainingSessions),
    activeCount: activeSessions.length,
    remainingCount: remainingSessions.length,
  };
}
