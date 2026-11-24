import type { SessionEntry } from "tddy-client-web/gen/session_pb";
import { sessionStackForest, type SubagentSessionNode } from "../agentTree";
import { connectionStatusForSession } from "./connectionStatusForSession";
import { sortSessionsByCreation, sortSessionsByLastTouched } from "./sessionSort";

/** Result of {@link groupSessionsByStack}: one partition's rows, in drawer order. */
export interface SessionStackGroupResult {
  /**
   * Top-level rows: every session no listed session started (a plain session, an orchestrator, or
   * a child whose orchestrator is not in the list). Each carries the sessions it started beneath
   * it, recursively, so a started session renders right after its starter.
   */
  rows: SubagentSessionNode[];
}

/** How a partition orders its sessions; the same order applies to roots and to each set of siblings. */
export type SessionOrder = (sessions: SessionEntry[]) => SessionEntry[];

/**
 * Folds sessions into the drawer's stack rows (`orchestratorSessionId`), ordered by `order` —
 * newest-started first unless told otherwise. A starter keeps its own place among the top-level
 * rows, and the sessions it started follow it in the same order.
 */
export function groupSessionsByStack(
  sessions: SessionEntry[],
  order: SessionOrder = sortSessionsByCreation,
): SessionStackGroupResult {
  return { rows: sessionStackForest(order(sessions)) };
}

/** An active / remaining split of a session list, each side independently stack-folded. */
export interface SessionActivityPartitions {
  /** Active partition (green/yellow dots), newest-started first. */
  active: SessionStackGroupResult;
  /** Remaining partition (grey/disconnected dots), most recently touched first. */
  remaining: SessionStackGroupResult;
  /** Raw count of sessions routed to the active partition. */
  activeCount: number;
  /** Raw count of sessions routed to the remaining partition. */
  remainingCount: number;
}

/**
 * Splits sessions into an *active* partition (dot is green or yellow, i.e.
 * {@link connectionStatusForSession} is not `"disconnected"`) and a *remaining* partition
 * (grey/disconnected), then stack-folds each side independently: active newest-started first,
 * remaining by the later of last activity and last update, so the session that stopped most
 * recently leads.
 *
 * Because the raw list is filtered *before* folding, a stack whose sessions differ in activity
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
    active: groupSessionsByStack(activeSessions, sortSessionsByCreation),
    remaining: groupSessionsByStack(remainingSessions, sortSessionsByLastTouched),
    activeCount: activeSessions.length,
    remainingCount: remainingSessions.length,
  };
}
