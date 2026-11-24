import type { SessionEntry } from "tddy-client-web/gen/session_pb";

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

/**
 * Fold the whole flat list into a forest: every session no listed session started is a root, and
 * each started session sits beneath its starter, following `orchestrator_session_id` recursively.
 * The sessions drawer renders it, one partition at a time.
 *
 * Unlike {@link subagentSessionNodes}, nothing is dropped — every session lands exactly once. A
 * session naming itself, or one whose orchestrator is not listed, is a root. Sessions whose
 * orchestrator links only form a cycle have no root to hang from; each such cycle is entered at its
 * first member in input order and appended after the other roots, so the drawer still lists them.
 *
 * Roots and siblings keep the input list's order: the caller sorts first.
 *
 * Feature: docs/ft/web/session-drawer.md § Row order.
 */
export function sessionStackForest(sessions: ReadonlyArray<SessionEntry>): SubagentSessionNode[] {
  const listed = new Set(sessions.map((session) => session.sessionId));
  const placed = new Set<string>();

  const fold = (session: SessionEntry): SubagentSessionNode => {
    placed.add(session.sessionId);
    const children = sessions.filter(
      (child) => child.orchestratorSessionId === session.sessionId && !placed.has(child.sessionId),
    );
    return { session, children: children.map(fold) };
  };

  const isRoot = (session: SessionEntry) =>
    session.orchestratorSessionId === "" ||
    session.orchestratorSessionId === session.sessionId ||
    !listed.has(session.orchestratorSessionId);

  const roots = sessions.filter(isRoot).map(fold);
  for (const session of sessions) {
    if (!placed.has(session.sessionId)) roots.push(fold(session));
  }
  return roots;
}
