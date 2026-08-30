import type { SessionEntry } from "../../gen/connection_pb";

/**
 * The subagent sessions beneath one session, as a tree.
 *
 * A node carries the whole `SessionEntry` rather than an id: a row renders the session's agent,
 * model and inferred status, and an id alone would force it to re-scan the list to do so.
 */
export interface SubagentSessionNode {
  readonly session: SessionEntry;
  readonly children: ReadonlyArray<SubagentSessionNode>;
}

/**
 * Fold the flat `ListSessions` list into the sessions spawned beneath `rootSessionId`, following
 * `orchestrator_session_id` recursively.
 *
 * The root itself is never one of its own subagents, a session naming *itself* as its orchestrator
 * is dropped, and a session whose orchestrator is not reachable from the root does not appear at
 * all — promoting an orphan to the top would claim this agent spawned it.
 *
 * The ids already on the current branch are carried down so that orchestrator links which form a
 * cycle terminate: the branch ends where it would otherwise descend into a session it already
 * holds. `ListSessions` is assembled from several hosts' answers, so a cycle is a shape the web has
 * to survive rather than one it can assume away.
 *
 * Siblings keep the input list's order, so two folds over one list agree.
 *
 * Feature: docs/ft/daemon/session-agent-roster.md § The Agents tab (AC53a, AC53e).
 */
export function subagentSessionNodes(
  sessions: ReadonlyArray<SessionEntry>,
  rootSessionId: string,
): SubagentSessionNode[] {
  const childrenOf = (parentId: string, onBranch: ReadonlySet<string>): SubagentSessionNode[] =>
    sessions
      .filter(
        (session) =>
          session.orchestratorSessionId === parentId && !onBranch.has(session.sessionId),
      )
      .map((session) => ({
        session,
        children: childrenOf(session.sessionId, new Set([...onBranch, session.sessionId])),
      }));

  return childrenOf(rootSessionId, new Set([rootSessionId]));
}
